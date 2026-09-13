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
    pub source_account_id: String,
    pub source_dir: String,
    pub source_root: String,
    /// 文件附件候选数（msg/file）
    pub files_count_estimated: usize,
    /// 视频候选数（msg/video 下的 .mp4）
    pub videos_count_estimated: usize,
    /// 图片候选数（msg/attach/**/Img）
    pub images_count_estimated: usize,
}

/// 立即扫描时用于定位微信账号的目录与账号标识。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WechatAccountTargetDto {
    pub source_root: String,
    pub source_account_id: String,
}

/// 扫描请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequestDto {
    pub target_accounts: Vec<WechatAccountTargetDto>,
    /// true 时忽略检查点做全量发现；默认 false 使用增量扫描
    #[serde(default)]
    pub full_scan: bool,
}

/// 扫描总结报告
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResultDto {
    pub total_discovered: usize,
    pub total_new_objects: usize,
    pub total_skipped: usize,
    pub duration_ms: u128,
    /// 图片候选发现数
    #[serde(default)]
    pub images_discovered: usize,
    /// 图片解密校验成功数
    #[serde(default)]
    pub images_prepared: usize,
    /// 参数不可用数
    #[serde(default)]
    pub images_parameters_unavailable: usize,
    /// 未支持格式数
    #[serde(default)]
    pub images_unsupported: usize,
    /// 校验失败数
    #[serde(default)]
    pub images_invalid: usize,
    /// 逻辑图片组数
    #[serde(default)]
    pub images_logical_groups: usize,
}

/// 查询过滤参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQueryDto {
    pub keyword: Option<String>,
    pub category: Option<String>,
    pub source_type: Option<String>,
    pub source_account_id: Option<String>,
    pub source_conversation_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// 来源账号映射展示对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAccountDto {
    pub source_type: String,
    pub source_account_id: String,
    pub source_name: Option<String>,
    pub display_name: Option<String>,
    pub effective_name: String,
    pub is_favorite: bool,
    pub record_count: usize,
}

/// 来源聊天映射展示对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConversationDto {
    pub source_type: String,
    pub source_account_id: String,
    pub source_conversation_id: String,
    pub source_name: Option<String>,
    pub display_name: Option<String>,
    pub effective_name: String,
    pub is_favorite: bool,
    pub record_count: usize,
}

/// 来源账号名称/收藏更新请求；来源类型和 ID 只用于定位，不可被更新。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSourceAccountDto {
    pub source_type: String,
    pub source_account_id: String,
    pub display_name: Option<String>,
    pub is_favorite: bool,
}

/// 来源聊天名称/收藏更新请求；来源类型、账号 ID 和聊天 ID 只用于定位。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSourceConversationDto {
    pub source_type: String,
    pub source_account_id: String,
    pub source_conversation_id: String,
    pub display_name: Option<String>,
    pub is_favorite: bool,
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
    /// 基础网络连通
    pub reachable: bool,
    /// 认证与上传/回读是否通过
    pub authenticated: bool,
    /// 目录创建能力
    pub support_mkcol: bool,
    /// 暂存移动能力
    pub support_move: bool,
    /// 探测说明或错误信息
    pub message: String,
    /// 探测耗时（毫秒）
    pub duration_ms: u128,
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
pub mod pipeline;
pub mod scan;
pub mod settings;
pub mod sources;
pub mod sync;
pub mod tasks;
pub mod webdav;
