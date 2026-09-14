// ChatVault Windows 企业微信目录自动探测模块
// 负责定位 WXWork 数据根、过滤非账号目录并枚举有效账号。
use chatvault_core::error::{ChatVaultError, Result};
use std::path::{Path, PathBuf};

/// 判断路径是否为符号链接或 Windows 重解析点；遍历时一律拒绝。
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

/// 探测到的企业微信账号信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WxWorkAccount {
    /// 账号标识（数字目录名，如 1688856363248674）
    pub source_account_id: String,
    /// 该账号数据根目录
    pub root_dir: PathBuf,
    /// 聊天附件目录（通常为 `root_dir/Cache/File`）
    pub files_dir: PathBuf,
    /// 视频本体目录（通常为 `root_dir/Cache/Video`）
    pub video_dir: PathBuf,
}

/// 企业微信探测器
pub struct WxWorkDetector;

/// 非账号噪声目录名（CEF/全局缓存等），一律排除。
const NOISE_DIRS: &[&str] = &[
    "default",
    "global",
    "profiles",
    "dictionaries",
    "graphitedawncache",
    "grshadercache",
    "shadercache",
    "safe browsing",
    "segmentation_platform",
    "qtcef",
];

/// 判断目录名是否为噪声目录。
fn is_noise_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    NOISE_DIRS.iter().any(|noise| lower == *noise)
}

impl WxWorkDetector {
    /// 校验手动选择的企业微信数据根目录。
    ///
    /// 职责: 确认路径是现有目录且目录名为 `WXWork`，避免把账号目录或
    /// `Cache/File` 附件目录误当成根目录。
    pub fn validate_root<P: AsRef<Path>>(root: P) -> Result<PathBuf> {
        let path = root.as_ref();
        if !path.is_dir() || is_link_or_reparse(path) {
            return Err(ChatVaultError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        let is_wxwork_root = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("WXWork"))
            .unwrap_or(false);
        if !is_wxwork_root {
            return Err(ChatVaultError::SourceParse(
                "请选择企业微信的 WXWork 根目录，不能选择账号或 Cache 子目录".to_string(),
            ));
        }

        Ok(path.to_path_buf())
    }

    /// 默认数据根候选：Documents\WXWork 及跨盘符常见位置。
    pub fn candidate_roots() -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(doc_dir) = dirs::document_dir() {
            candidates.push(doc_dir.join("WXWork"));
        }
        for drive in ["D:\\", "E:\\", "F:\\"] {
            candidates.push(PathBuf::from(drive).join("WXWork"));
            candidates.push(PathBuf::from(drive).join("Documents").join("WXWork"));
        }
        candidates
    }

    /// 自动发现系统中的企业微信根目录。
    pub fn detect_root() -> Result<PathBuf> {
        for candidate in Self::candidate_roots() {
            if candidate.exists() && candidate.is_dir() {
                return Ok(candidate);
            }
        }
        Err(ChatVaultError::SourceParse(
            "未检测到企业微信数据目录 (WXWork)，请确认企业微信是否已安装登录，或手动指定目录"
                .to_string(),
        ))
    }

    /// 列出根目录下所有有效企业微信账号。
    ///
    /// 职责: 过滤 CEF/全局噪声目录，要求账号目录具备 `Cache` 或 `Data` 子目录
    /// 或 `Config.cfg`（历史空账号仅有 Config.cfg 时仍列出）。
    pub fn find_accounts<P: AsRef<Path>>(root: P) -> Result<Vec<WxWorkAccount>> {
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
            if is_noise_dir(dir_name) {
                continue;
            }

            let cache_dir = path.join("Cache");
            let data_dir = path.join("Data");
            let config_file = path.join("Config.cfg");
            let has_cache = cache_dir.is_dir();
            let has_data = data_dir.is_dir();
            let has_config = config_file.is_file();
            if !has_cache && !has_data && !has_config {
                continue;
            }

            let files_dir = cache_dir.join("File");
            let video_dir = cache_dir.join("Video");
            accounts.push(WxWorkAccount {
                source_account_id: dir_name.to_string(),
                root_dir: path,
                files_dir,
                video_dir,
            });
        }

        Ok(accounts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_roots_not_empty() {
        assert!(!WxWorkDetector::candidate_roots().is_empty());
    }

    #[test]
    fn validate_root_requires_wxwork_directory() {
        let base =
            std::env::temp_dir().join(format!("chatvault-wxwork-root-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("not-wxwork")).unwrap();
        std::fs::create_dir_all(base.join("WXWork")).unwrap();

        assert!(WxWorkDetector::validate_root(base.join("missing")).is_err());
        assert!(WxWorkDetector::validate_root(base.join("not-wxwork")).is_err());
        assert_eq!(
            WxWorkDetector::validate_root(base.join("WXWork")).unwrap(),
            base.join("WXWork")
        );

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn find_accounts_filters_noise_and_requires_marker() {
        let base = std::env::temp_dir().join(format!(
            "chatvault-wxwork-accounts-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("WXWork");
        std::fs::create_dir_all(root.join("Default")).unwrap();
        std::fs::create_dir_all(root.join("Global")).unwrap();
        std::fs::create_dir_all(root.join("1688850000000001")).unwrap();
        std::fs::create_dir_all(root.join("1688850000000002").join("Cache").join("File")).unwrap();
        std::fs::create_dir_all(root.join("1688850000000003")).unwrap();
        std::fs::write(root.join("1688850000000003").join("Config.cfg"), b"{}").unwrap();

        let accounts = WxWorkDetector::find_accounts(&root).unwrap();
        let ids: Vec<_> = accounts
            .iter()
            .map(|a| a.source_account_id.as_str())
            .collect();
        assert!(ids.contains(&"1688850000000002"));
        assert!(ids.contains(&"1688850000000003"));
        assert!(!ids.contains(&"Default"));
        assert!(!ids.contains(&"Global"));
        assert!(!ids.contains(&"1688850000000001"));

        std::fs::remove_dir_all(&base).unwrap();
    }
}
