//! # Windows 微信 4.x 目录自动探测模块
//!
//! 自动探测 Windows 系统中微信 4.x (`xwechat_files`) 的数据根目录，
//! 并识别所有已登录过的独立用户账号及其存储路径。

use chatvault_core::error::{ChatVaultError, Result};
use std::path::{Path, PathBuf};

/// 判断路径是否为符号链接或 Windows 重解析点；遍历微信目录时一律拒绝。
pub(crate) fn is_link_or_reparse(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return true;
    };
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// 探测到的微信 4.x 账号信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeChatAccount {
    /// 微信账号标识 (例如 wxid_xxx 或微信号)
    pub source_account_id: String,
    /// 该账号数据根目录绝对路径
    pub root_dir: PathBuf,
    /// 聊天附件文件所在目录 (通常为 `root_dir/msg/file`)
    pub files_dir: PathBuf,
    /// 视频本体目录 (`msg/video`)；缺失时该路径仍给出，由扫描层按空目录处理
    pub video_dir: PathBuf,
}

/// 微信 4.x 探测器
pub struct WeChat4Detector;

impl WeChat4Detector {
    /// 校验手动选择的微信 4.x 数据根目录。
    ///
    /// 职责: 确认路径是现有目录且目录名为 `xwechat_files`，避免把账号目录或
    /// `msg/file` 附件目录误当成微信根目录。输入为用户选择的目录路径，输出为
    /// 可继续枚举账号的规范路径。
    pub fn validate_root<P: AsRef<Path>>(root: P) -> Result<PathBuf> {
        let path = root.as_ref();
        if !path.is_dir() || is_link_or_reparse(path) {
            return Err(ChatVaultError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        let is_xwechat_root = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("xwechat_files"))
            .unwrap_or(false);
        if !is_xwechat_root {
            return Err(ChatVaultError::SourceParse(
                "请选择微信 4.x 的 xwechat_files 根目录，不能选择账号或 msg/file 子目录"
                    .to_string(),
            ));
        }

        Ok(path.to_path_buf())
    }

    /// 获取默认的微信 4.x 数据根目录候选列表
    ///
    /// 职责: 返回标准 `Documents\xwechat_files` 以及跨盘符常见位置
    /// 输出: 潜在的根目录路径列表
    pub fn candidate_roots() -> Vec<PathBuf> {
        let mut candidates = Vec::new();

        // 默认文档目录: C:\Users\<Username>\Documents\xwechat_files
        if let Some(doc_dir) = dirs::document_dir() {
            candidates.push(doc_dir.join("xwechat_files"));
        }

        // 备用盘符探测 (D, E, F 等自定义盘符根目录)
        for drive in ["D:\\", "E:\\", "F:\\"] {
            candidates.push(PathBuf::from(drive).join("xwechat_files"));
            candidates.push(PathBuf::from(drive).join("Documents").join("xwechat_files"));
        }

        candidates
    }

    /// 自动发现系统中的微信 4.x 根目录
    ///
    /// 职责: 遍历候选路径，查找首个实际存在的有效微信 4.x 数据目录
    /// 输出: `Result<PathBuf>`
    pub fn detect_root() -> Result<PathBuf> {
        for candidate in Self::candidate_roots() {
            if candidate.exists() && candidate.is_dir() {
                return Ok(candidate);
            }
        }

        Err(ChatVaultError::SourceParse(
            "未检测到微信 4.x 数据目录 (xwechat_files)，请确认微信 4.x 是否已安装登录，或手动指定目录".to_string(),
        ))
    }

    /// 列出微信 4.x 根目录下所有有效的微信账号
    ///
    /// 职责: 枚举根目录子文件夹，过滤非用户账号目录（如 all_users, Backup），
    ///       并验证账号目录下是否存在 `msg` 或 `msg/file`。
    /// 输入:
    ///   - `root`: 微信 4.x 数据根目录 (例如 `Documents/xwechat_files`)
    ///     输出: `Result<Vec<WeChatAccount>>`
    pub fn find_accounts<P: AsRef<Path>>(root: P) -> Result<Vec<WeChatAccount>> {
        let r = root.as_ref();
        if !r.exists() || is_link_or_reparse(r) {
            return Err(ChatVaultError::FileNotFound {
                path: r.display().to_string(),
            });
        }

        let mut accounts = Vec::new();
        let entries = std::fs::read_dir(r)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if is_link_or_reparse(&path) || !path.is_dir() {
                continue;
            }

            let dir_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name,
                None => continue,
            };

            // 过滤系统或备份目录
            let lower = dir_name.to_lowercase();
            if lower == "all_users" || lower == "backup" || lower == "temp" {
                continue;
            }

            let msg_dir = path.join("msg");
            let files_dir = msg_dir.join("file");
            let video_dir = msg_dir.join("video");

            // 只要存在 msg 目录，即使当前还没有收到 file，也属于合法账号
            if msg_dir.exists() {
                accounts.push(WeChatAccount {
                    source_account_id: dir_name.to_string(),
                    root_dir: path,
                    files_dir,
                    video_dir,
                });
            }
        }

        Ok(accounts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_roots_not_empty() {
        let roots = WeChat4Detector::candidate_roots();
        assert!(!roots.is_empty());
    }

    #[test]
    fn validate_root_requires_xwechat_files_directory() {
        let base =
            std::env::temp_dir().join(format!("chatvault-wechat-root-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("not-wechat")).unwrap();
        std::fs::create_dir_all(base.join("xwechat_files")).unwrap();

        assert!(WeChat4Detector::validate_root(base.join("missing")).is_err());
        assert!(WeChat4Detector::validate_root(base.join("not-wechat")).is_err());
        assert_eq!(
            WeChat4Detector::validate_root(base.join("xwechat_files")).unwrap(),
            base.join("xwechat_files")
        );

        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn find_accounts_requires_msg_directory() {
        let base = std::env::temp_dir().join(format!(
            "chatvault-wechat-accounts-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("xwechat_files");
        std::fs::create_dir_all(root.join("wxid_without_msg")).unwrap();
        std::fs::create_dir_all(root.join("wxid_with_msg").join("msg")).unwrap();

        let accounts = WeChat4Detector::find_accounts(&root).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].source_account_id, "wxid_with_msg");

        std::fs::remove_dir_all(base).unwrap();
    }
}
