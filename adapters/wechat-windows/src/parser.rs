//! # 微信 4.x 附件解析与提取模块
//!
//! 负责递归遍历微信 4.x 账号下的 `msg/file` 目录，提取文件大小、修改时间、
//! 原文件名、所属账号与可验证的聊天目录标识，打包为标准的 `DiscoveredFile`
//! 实例供后续入库与归档。

use crate::detector::WeChatAccount;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::{DiscoveredFile, WECHAT_WINDOWS_4_SOURCE_TYPE};
use chatvault_scanner::strategy::MtimeAtDepthStrategy;
use chatvault_scanner::walker::{scan_directory_with_strategy, ScanOptions};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::time::SystemTime;

/// 微信 4.x 文件解析器
pub struct WeChat4Parser;

impl WeChat4Parser {
    /// 全量扫描指定微信账号的附件目录
    pub fn parse_account_files(account: &WeChatAccount) -> Result<Vec<DiscoveredFile>> {
        Self::parse_account_files_since(account, None)
    }

    /// 按微信 4.x 固定目录结构增量发现附件；None 为全量
    ///
    /// 职责: 扫描 `account.files_dir`（即 `msg/file/`）下的文件，提取元数据
    /// 输入:
    ///   - `account`: 微信账号结构体
    ///   - `since`: 增量起点；月份目录 mtime 不晚于该时刻时跳过对应子树
    ///     输出: `Result<Vec<DiscoveredFile>>`
    pub fn parse_account_files_since(
        account: &WeChatAccount,
        since: Option<SystemTime>,
    ) -> Result<Vec<DiscoveredFile>> {
        if !account.files_dir.exists() {
            tracing::info!(
                "账号 {} 的附件目录尚未生成: {}",
                account.source_account_id,
                account.files_dir.display()
            );
            return Ok(Vec::new());
        }

        Self::parse_folder_since(&account.files_dir, Some(&account.source_account_id), since)
    }

    /// 全量扫描指定微信账号的视频目录（仅 `.mp4` 本体）
    pub fn parse_account_videos(account: &WeChatAccount) -> Result<Vec<DiscoveredFile>> {
        Self::parse_account_videos_since(account, None)
    }

    /// 增量发现 `msg/video` 下的 `.mp4` 视频本体
    ///
    /// 职责: 跳过 `.jpg` / `_thumb.jpg` 与 0 字节；会话 ID 恒为 None
    /// 输入:
    ///   - `account`: 微信账号结构体
    ///   - `since`: 增量起点；月份目录 mtime 不晚于该时刻时跳过对应子树
    pub fn parse_account_videos_since(
        account: &WeChatAccount,
        since: Option<SystemTime>,
    ) -> Result<Vec<DiscoveredFile>> {
        if !account.video_dir.exists() {
            tracing::info!(
                "账号 {} 的视频目录尚未生成: {}",
                account.source_account_id,
                account.video_dir.display()
            );
            return Ok(Vec::new());
        }

        let options = ScanOptions {
            max_depth: None,
            skip_hidden: true,
            min_size: 1,
        };
        let strategy = MtimeAtDepthStrategy::new(1);
        let paths = scan_directory_with_strategy(&account.video_dir, &options, since, &strategy)?;
        let mut discovered = Vec::with_capacity(paths.len());

        for path in paths {
            if !Self::is_mp4_file(&path) {
                continue;
            }
            let metadata = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("无法读取视频元数据 {}: {}", path.display(), e);
                    continue;
                }
            };
            let file_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };
            let modified_system = metadata.modified().map_err(ChatVaultError::Io)?;
            let modified_time: DateTime<Utc> = modified_system.into();

            discovered.push(DiscoveredFile {
                source_type: WECHAT_WINDOWS_4_SOURCE_TYPE.to_string(),
                source_account_id: Some(account.source_account_id.clone()),
                absolute_path: path.to_string_lossy().to_string(),
                file_name,
                file_size: metadata.len(),
                modified_time,
                // 视频目录无会话子目录，不把月份目录伪造成会话 ID
                source_conversation_id: None,
            });
        }

        Ok(discovered)
    }

    /// 判断是否为 `.mp4` 普通文件（大小写不敏感）
    fn is_mp4_file(path: &Path) -> bool {
        path.is_file()
            && path
                .extension()
                .and_then(|s| s.to_str())
                .map(|e| e.eq_ignore_ascii_case("mp4"))
                .unwrap_or(false)
    }

    /// 从任意给定的微信 4.x 目录提取文件（全量）
    pub fn parse_folder<P: AsRef<Path>>(
        folder: P,
        source_account_id: Option<&str>,
    ) -> Result<Vec<DiscoveredFile>> {
        Self::parse_folder_since(folder, source_account_id, None)
    }

    /// 从微信 4.x 文件路径中提取可验证的聊天 ID。
    ///
    /// 职责: 仅识别 `msg/file/YYYY-MM/<conversation_id>/<file>` 形式的稳定聊天目录，
    ///       不把月份目录或平铺文件名误认为聊天 ID。
    /// 输入:
    ///   - `files_root`: 微信账号的 `msg/file` 根目录
    ///   - `file_path`: 待解析的文件路径
    ///     输出: 聊天目录名；标准 `msg/file/YYYY-MM/<file>` 平铺布局返回 `None`
    pub fn conversation_id_for_path<P: AsRef<Path>, Q: AsRef<Path>>(
        files_root: P,
        file_path: Q,
    ) -> Option<String> {
        let relative = file_path.as_ref().strip_prefix(files_root.as_ref()).ok()?;
        let mut components = relative.components();
        let month = components.next()?.as_os_str().to_str()?;
        if !is_month_directory(month) {
            return None;
        }

        let conversation = components.next()?.as_os_str().to_str()?;
        // 只有月份目录、聊天目录和文件三层时才认定第二层是聊天 ID；
        // 更深层路径仍可保留第一层聊天目录，供未来布局扩展使用。
        if components.next().is_none() || conversation.is_empty() {
            return None;
        }

        Some(conversation.to_string())
    }

    /// 按微信 4.x 固定目录策略增量提取文件并附带上下文标签
    pub fn parse_folder_since<P: AsRef<Path>>(
        folder: P,
        source_account_id: Option<&str>,
        since: Option<SystemTime>,
    ) -> Result<Vec<DiscoveredFile>> {
        let f = folder.as_ref();
        let options = ScanOptions {
            max_depth: None,
            skip_hidden: true,
            min_size: 1, // 微信可能有占位 0 字节文件，跳过空文件
        };

        // 微信 4.x 的 files 根目录下固定按月份分目录，深度 1 是安全的裁剪边界。
        let strategy = MtimeAtDepthStrategy::new(1);
        let paths = scan_directory_with_strategy(f, &options, since, &strategy)?;
        let mut discovered = Vec::with_capacity(paths.len());

        for path in paths {
            let metadata = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("无法读取文件元数据 {}: {}", path.display(), e);
                    continue;
                }
            };

            let file_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            let modified_system = metadata.modified().map_err(ChatVaultError::Io)?;
            let modified_time: DateTime<Utc> = modified_system.into();

            // 只有存在明确聊天目录时才写入聊天 ID；标准月份平铺文件保持未知。
            let source_conversation_id = Self::conversation_id_for_path(f, &path);
            discovered.push(DiscoveredFile {
                source_type: WECHAT_WINDOWS_4_SOURCE_TYPE.to_string(),
                source_account_id: source_account_id.map(|s| s.to_string()),
                absolute_path: path.to_string_lossy().to_string(),
                file_name,
                file_size: metadata.len(),
                modified_time,
                source_conversation_id,
            });
        }

        Ok(discovered)
    }
}

/// 判断路径第一层是否为微信 4.x 文件目录使用的月份目录。
fn is_month_directory(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 7
        && bytes[4] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || byte.is_ascii_digit())
        && matches!(name[5..7].parse::<u8>(), Ok(1..=12))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn extracts_conversation_id_from_nested_file_layout() {
        let root = Path::new(r"C:\xwechat_files\wxid_test\msg\file");
        let file = root.join("2026-09").join("wxid_friend").join("report.pdf");

        assert_eq!(
            WeChat4Parser::conversation_id_for_path(root, &file).as_deref(),
            Some("wxid_friend")
        );
    }

    #[test]
    fn flat_month_layout_has_no_conversation_id() {
        let root = Path::new(r"C:\xwechat_files\wxid_test\msg\file");
        let file = root.join("2026-09").join("report.pdf");

        assert_eq!(WeChat4Parser::conversation_id_for_path(root, &file), None);
    }

    #[test]
    fn rejects_non_month_directory_as_conversation_context() {
        let root = Path::new(r"C:\xwechat_files\wxid_test\msg\file");
        let file = root.join("archive").join("wxid_friend").join("report.pdf");

        assert_eq!(WeChat4Parser::conversation_id_for_path(root, &file), None);
    }

    #[test]
    fn parser_emits_conversation_id_for_nested_layout() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("chatvault_wechat_parser_{unique}"));
        let file = root.join("2026-09").join("wxid_friend").join("report.pdf");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"report").unwrap();

        let discovered = WeChat4Parser::parse_folder(&root, Some("wxid_owner")).unwrap();

        assert_eq!(discovered.len(), 1);
        assert_eq!(
            discovered[0].source_conversation_id.as_deref(),
            Some("wxid_friend")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    fn make_account(base: &Path) -> WeChatAccount {
        let root_dir = base.join("xwechat_files").join("wxid_test");
        WeChatAccount {
            source_account_id: "wxid_test".to_string(),
            files_dir: root_dir.join("msg").join("file"),
            video_dir: root_dir.join("msg").join("video"),
            root_dir,
        }
    }

    #[test]
    fn video_parser_only_collects_mp4() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("chatvault_wechat_video_{unique}"));
        let account = make_account(&base);
        let month = account.video_dir.join("2026-09");
        std::fs::create_dir_all(&month).unwrap();
        std::fs::write(month.join("abc123.mp4"), b"fake-mp4-bytes").unwrap();
        std::fs::write(month.join("abc123.jpg"), b"cover").unwrap();
        std::fs::write(month.join("abc123_thumb.jpg"), b"thumb").unwrap();
        std::fs::write(month.join("empty.mp4"), b"").unwrap();

        let discovered = WeChat4Parser::parse_account_videos(&account).unwrap();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].file_name, "abc123.mp4");
        assert!(discovered[0].source_conversation_id.is_none());
        assert_eq!(
            discovered[0].source_account_id.as_deref(),
            Some("wxid_test")
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn video_parser_handles_missing_directory() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("chatvault_wechat_video_miss_{unique}"));
        let account = make_account(&base);

        let discovered = WeChat4Parser::parse_account_videos(&account).unwrap();
        assert!(discovered.is_empty());
    }
}
