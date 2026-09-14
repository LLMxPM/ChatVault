// ChatVault 企业微信附件与视频解析模块
// 负责遍历 Cache/File 与 Cache/Video，生成标准 DiscoveredFile。
use crate::detector::WxWorkAccount;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::{DiscoveredFile, WXWORK_WINDOWS_SOURCE_TYPE};
use chatvault_scanner::strategy::MtimeAtDepthStrategy;
use chatvault_scanner::walker::{scan_directory_with_strategy, ScanOptions};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::time::SystemTime;

/// 企业微信文件解析器
pub struct WxWorkParser;

impl WxWorkParser {
    /// 全量扫描账号附件目录
    pub fn parse_account_files(account: &WxWorkAccount) -> Result<Vec<DiscoveredFile>> {
        Self::parse_account_files_since(account, None)
    }

    /// 增量扫描账号附件目录；None 为全量
    ///
    /// 输入:
    ///   - `account`: 企业微信账号
    ///   - `since`: 增量起点；月份目录 mtime 不晚于该时刻时跳过对应子树
    pub fn parse_account_files_since(
        account: &WxWorkAccount,
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

    /// 全量扫描账号视频目录（仅 `.mp4`）
    pub fn parse_account_videos(account: &WxWorkAccount) -> Result<Vec<DiscoveredFile>> {
        Self::parse_account_videos_since(account, None)
    }

    /// 增量扫描账号视频目录
    pub fn parse_account_videos_since(
        account: &WxWorkAccount,
        since: Option<SystemTime>,
    ) -> Result<Vec<DiscoveredFile>> {
        Self::parse_videos_folder_since(&account.video_dir, Some(&account.source_account_id), since)
    }

    /// 从附件根目录提取文件
    pub fn parse_folder<P: AsRef<Path>>(
        folder: P,
        source_account_id: Option<&str>,
    ) -> Result<Vec<DiscoveredFile>> {
        Self::parse_folder_since(folder, source_account_id, None)
    }

    /// 增量提取附件；跳过 Temp 目录与 0 字节文件
    ///
    /// 输入:
    ///   - `folder`: `Cache/File` 根目录
    ///   - `source_account_id`: 归属账号
    ///   - `since`: 增量起点；月份目录 mtime 不晚于该时刻时跳过对应子树
    pub fn parse_folder_since<P: AsRef<Path>>(
        folder: P,
        source_account_id: Option<&str>,
        since: Option<SystemTime>,
    ) -> Result<Vec<DiscoveredFile>> {
        let f = folder.as_ref();
        let options = ScanOptions {
            max_depth: None,
            skip_hidden: true,
            min_size: 1,
        };
        // 企业微信 Cache/File 固定按月份分目录，深度 1 是安全的裁剪边界。
        let strategy = MtimeAtDepthStrategy::new(1);
        let paths = scan_directory_with_strategy(f, &options, since, &strategy)?;
        let mut discovered = Vec::with_capacity(paths.len());

        for path in paths {
            if is_under_temp(f, &path) {
                continue;
            }
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

            discovered.push(DiscoveredFile {
                source_type: WXWORK_WINDOWS_SOURCE_TYPE.to_string(),
                source_account_id: source_account_id.map(|s| s.to_string()),
                absolute_path: path.to_string_lossy().to_string(),
                file_name,
                file_size: metadata.len(),
                modified_time,
                // 月份目录下为平铺文件，无可验证会话子目录
                source_conversation_id: None,
            });
        }

        Ok(discovered)
    }

    /// 增量发现视频目录下的 `.mp4` 本体；跳过 Temp
    pub fn parse_videos_folder_since<P: AsRef<Path>>(
        video_dir: P,
        source_account_id: Option<&str>,
        since: Option<SystemTime>,
    ) -> Result<Vec<DiscoveredFile>> {
        let f = video_dir.as_ref();
        if !f.exists() {
            tracing::info!("视频目录尚未生成: {}", f.display());
            return Ok(Vec::new());
        }

        let options = ScanOptions {
            max_depth: None,
            skip_hidden: true,
            min_size: 1,
        };
        let strategy = MtimeAtDepthStrategy::new(1);
        let paths = scan_directory_with_strategy(f, &options, since, &strategy)?;
        let mut discovered = Vec::with_capacity(paths.len());

        for path in paths {
            if is_under_temp(f, &path) || !is_mp4_file(&path) {
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
                source_type: WXWORK_WINDOWS_SOURCE_TYPE.to_string(),
                source_account_id: source_account_id.map(|s| s.to_string()),
                absolute_path: path.to_string_lossy().to_string(),
                file_name,
                file_size: metadata.len(),
                modified_time,
                source_conversation_id: None,
            });
        }

        Ok(discovered)
    }
}

/// 判断路径是否位于 `Temp` 子目录下（企业微信下载中转目录）。
fn is_under_temp(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    relative.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s.eq_ignore_ascii_case("Temp"))
            .unwrap_or(false)
    })
}

/// 判断是否为 `.mp4` 普通文件。
fn is_mp4_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.eq_ignore_ascii_case("mp4"))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn make_account(base: &Path) -> WxWorkAccount {
        let root_dir = base.join("WXWork").join("1688850000000001");
        WxWorkAccount {
            source_account_id: "1688850000000001".to_string(),
            files_dir: root_dir.join("Cache").join("File"),
            video_dir: root_dir.join("Cache").join("Video"),
            root_dir,
        }
    }

    #[test]
    fn parser_skips_temp_and_collects_month_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("chatvault_wxwork_parser_{unique}"));
        let account = make_account(&base);
        let month = account.files_dir.join("2026-06");
        let temp = account.files_dir.join("Temp");
        std::fs::create_dir_all(&month).unwrap();
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(month.join("报告.docx"), b"doc").unwrap();
        std::fs::write(temp.join("partial.bin"), b"tmp").unwrap();
        std::fs::write(month.join("empty.bin"), b"").unwrap();

        let discovered = WxWorkParser::parse_account_files(&account).unwrap();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].file_name, "报告.docx");
        assert!(discovered[0].source_conversation_id.is_none());
        assert_eq!(
            discovered[0].source_account_id.as_deref(),
            Some("1688850000000001")
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn video_parser_only_collects_mp4_and_skips_temp() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("chatvault_wxwork_video_{unique}"));
        let account = make_account(&base);
        let month = account.video_dir.join("2026-03");
        let temp = account.video_dir.join("Temp");
        std::fs::create_dir_all(&month).unwrap();
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(month.join("video.mp4"), b"fake-mp4").unwrap();
        std::fs::write(month.join("video.jpg"), b"cover").unwrap();
        std::fs::write(temp.join("video.mp4"), b"partial").unwrap();

        let discovered = WxWorkParser::parse_account_videos(&account).unwrap();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].file_name, "video.mp4");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn handles_missing_directories() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("chatvault_wxwork_miss_{unique}"));
        let account = make_account(&base);
        assert!(WxWorkParser::parse_account_files(&account).unwrap().is_empty());
        assert!(WxWorkParser::parse_account_videos(&account).unwrap().is_empty());
    }
}
