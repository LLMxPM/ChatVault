// ChatVault 来源适配器探测与解析结果：账号信息与扫描编排共享。
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 账号勾选/立即扫描目标：以 (来源根, 账号 ID) 定位。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountTarget {
    pub source_root: String,
    pub source_account_id: String,
}

/// 来源适配器探测到的账号与媒体统计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAccountInfo {
    pub source_account_id: String,
    pub source_dir: String,
    pub source_root: String,
    pub files_count_estimated: usize,
    pub videos_count_estimated: usize,
}

/// 单账号媒体根路径（文件根 + 视频根）。
#[derive(Debug, Clone)]
pub struct AccountMediaRoots {
    pub source_account_id: String,
    pub root_dir: PathBuf,
    pub files_dir: PathBuf,
    pub video_dir: PathBuf,
}

/// 将路径是否落在勾选账号集合内；None 表示全选。
pub fn is_selected_account(
    targets: Option<&[AccountTarget]>,
    root: &std::path::Path,
    account_id: &str,
) -> bool {
    let Some(targets) = targets else {
        return true;
    };
    let root_key = chatvault_core::normalize_scan_key(&root.to_string_lossy());
    targets.iter().any(|target| {
        target.source_account_id == account_id
            && chatvault_core::normalize_scan_key(&target.source_root) == root_key
    })
}
