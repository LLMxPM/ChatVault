// ChatVault 桌面端 Tauri Command 命令集合
// 桥接前端 UI 与底层的微信探测、扫描器、数据库检索、文件定位和 WebDAV 归档模块

use crate::state::{setting_keys, AppState};
use adapter_generic_folder::GenericFolderParser;
use adapter_wechat_windows::{WeChat4Detector, WeChat4Parser};
use chatvault_index::category::determine_category;
use chatvault_index::query::{SearchFilter, SearchService};
use chatvault_index::IngestResult;
use chatvault_webdav::{CapabilityDetector, WebDavClient, WebDavConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tauri::State;

/// 前端微信账号信息契约
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WechatAccountDto {
    pub account_id: String,
    pub source_dir: String,
    pub files_count_estimated: usize,
}

/// 扫描请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequestDto {
    pub target_accounts: Vec<String>,
    pub custom_folders: Vec<String>,
}

/// 扫描总结报告
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResultDto {
    pub total_discovered: usize,
    pub total_new_objects: usize,
    pub total_skipped: usize,
    pub duration_ms: u128,
}

/// 文件记录呈现对象
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecordViewDto {
    pub record_id: String,
    pub object_id: String,
    pub original_name: String,
    pub file_size: u64,
    pub formatted_size: String,
    pub hash: String,
    pub source_type: String,
    pub account_id: Option<String>,
    pub file_time: Option<String>,
    pub discovered_at: String,
    pub original_path: String,
    pub category: String,
}

/// 查询过滤参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQueryDto {
    pub keyword: Option<String>,
    pub category: Option<String>,
    pub account_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// 存储去重统计数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatsDto {
    pub total_records: i64,
    pub unique_objects: i64,
    pub total_raw_bytes: u64,
    pub unique_bytes: u64,
    pub saved_bytes: u64,
    pub dedup_ratio_percent: f64,
    pub formatted_total_raw: String,
    pub formatted_saved_bytes: String,
}

/// WebDAV 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebdavConfigDto {
    pub url: String,
    pub username: String,
    pub password: Option<String>,
    pub vault_id: String,
}

/// WebDAV 远端能力探测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebdavCapabilityDto {
    pub reachable: bool,
    pub server_header: Option<String>,
    pub dav_compliance: Vec<String>,
    pub supports_lock: bool,
    pub message: String,
}

/// 归档执行报告
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveResultDto {
    pub uploaded_count: usize,
    pub verified_count: usize,
    pub failed_count: usize,
    pub duration_ms: u128,
}

/// 辅助函数：根据文件名后缀分类
/// 辅助函数：字节大小人类友好格式化
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub mod library;
pub mod scan;
pub mod settings;
pub mod sync;
pub mod tasks;
pub mod webdav;
