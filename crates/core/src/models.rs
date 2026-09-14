//! # 核心领域模型定义
//!
//! 定义 ChatVault 中的基础实体，包括资料库配置、内容对象、文件来源记录、
//! 本地文件关联、状态流转枚举以及由适配器发掘出的待处理文件。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 微信 4.x 文件来源适配器标识。
pub const WECHAT_WINDOWS_4_SOURCE_TYPE: &str = "wechat-windows-4";

/// 通用附件目录适配器标识。
pub const GENERIC_FOLDER_SOURCE_TYPE: &str = "generic-folder";

/// 企业微信 Windows 文件来源适配器标识。
pub const WXWORK_WINDOWS_SOURCE_TYPE: &str = "wxwork-windows";

/// 持久化的采集源配置。
///
/// 目录路径与适配器类型成对保存，扫描编排层据此选择对应适配器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectSource {
    /// 适配器来源类型，例如 `wechat-windows-4` 或 `generic-folder`。
    pub source_type: String,
    /// 用户选择的目录绝对路径。
    pub path: String,
    /// 微信是否识别视频（`msg/video`）；非微信来源忽略该字段。
    #[serde(default = "default_enable_videos")]
    pub enable_videos: bool,
}

/// 采集源视频识别的默认开关：默认开启，用户可显式关闭。
fn default_enable_videos() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{CollectSource, GENERIC_FOLDER_SOURCE_TYPE};

    #[test]
    fn collect_source_uses_camel_case_wire_fields() {
        let source = CollectSource {
            source_type: GENERIC_FOLDER_SOURCE_TYPE.to_string(),
            path: r"C:\attachments".to_string(),
            enable_videos: false,
        };
        let encoded = serde_json::to_string(&source).unwrap();
        assert_eq!(
            encoded,
            r#"{"sourceType":"generic-folder","path":"C:\\attachments","enableVideos":false}"#
        );
        assert_eq!(
            serde_json::from_str::<CollectSource>(&encoded).unwrap(),
            source
        );
        let legacy = serde_json::from_str::<CollectSource>(
            r#"{"sourceType":"generic-folder","path":"C:\\attachments"}"#,
        )
        .unwrap();
        assert!(legacy.enable_videos);
    }
}

/// 资料库核心配置元信息
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultConfig {
    /// 资料库唯一标识
    pub vault_id: String,
    /// 格式版本，当前首期固定为 1
    pub format_version: u32,
    /// 默认哈希算法，当前为 blake3
    pub hash_algorithm: String,
    /// 资料库创建时间
    pub created_at: DateTime<Utc>,
}

impl Default for VaultConfig {
    /// 创建默认配置的 VaultConfig
    ///
    /// 职责: 初始化默认的 Vault 配置实例
    /// 输出: 具有 `chatvault-` 前缀随机后缀与 blake3 算法的 VaultConfig
    fn default() -> Self {
        Self {
            vault_id: format!("chatvault-{}", uuid::Uuid::new_v4().simple()),
            format_version: 1,
            hash_algorithm: "blake3".to_string(),
            created_at: Utc::now(),
        }
    }
}

/// 内容寻址的文件对象 (不可变对象模型)
///
/// 相同内容的文件全局只保存一个 FileObject，以哈希为唯一键去重。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileObject {
    /// 对象标识，格式为 `算法:哈希值`，例如 `blake3:e3b0c44...`
    pub object_id: String,
    /// 原始十六进制哈希字符串
    pub hash: String,
    /// 文件大小（字节数）
    pub size: u64,
    /// 推断或检测到的 MIME 类型
    pub mime: String,
    /// 文件小写扩展名（不含点）
    pub extension: String,
    /// 首次创建时间
    pub created_at: DateTime<Utc>,
}

/// 文件来源记录模型
///
/// 一份相同的内容对象在不同时间、不同账号、不同路径下可能有多次接收记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    /// 记录唯一标识 (UUID)
    pub record_id: String,
    /// 关联的内容对象 ID (即 FileObject.object_id)
    pub object_id: String,
    /// 发现来源类型（例如 wechat-windows-4、generic-folder）
    pub source_type: String,
    /// 来源账号 ID（例如微信 wxid_xxx 或微信号）
    pub source_account_id: Option<String>,
    /// 会话标识（若可提取，否则为 None）
    pub source_conversation_id: Option<String>,
    /// 用户看到的文件原始文件名
    pub original_name: String,
    /// 文件的实际时间（通常为文件的最后修改时间 mtime）
    pub file_time: DateTime<Utc>,
    /// 时间的语义来源描述（例如 "mtime", "discovered"）
    pub time_source: String,
    /// 首次扫描发现时间
    pub discovered_at: DateTime<Utc>,
    /// 产生该记录的设备唯一标识
    pub device_id: String,
}

/// 本机文件映射关系模型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFile {
    /// 关联的 FileRecord 标识
    pub record_id: String,
    /// 本地完整绝对路径
    pub original_path: String,
    /// 本地下载/暂存缓存路径（若有）
    pub cache_path: Option<String>,
    /// 文件大小（字节）
    pub size: u64,
    /// 本地文件最后修改时间戳（毫秒）
    pub mtime_ms: i64,
    /// 本地可用状态
    pub availability: LocalAvailability,
}

/// 本地文件可用性状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalAvailability {
    /// 本地文件存在且可读
    Available,
    /// 本地源文件已被移动或删除
    Missing,
    /// 远端归档存在，但本地未缓存
    RemoteOnly,
}

/// 归档生命周期处理状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    /// 刚被发现
    Discovered,
    /// 稳定性检测通过（无追加写入）
    Stable,
    /// 已完成 BLAKE3 计算
    Hashed,
    /// 已入队等待上传
    Queued,
    /// 正在上传至 WebDAV
    Uploading,
    /// 正在进行远端回读哈希校验
    Verifying,
    /// 归档完成并通过校验
    BackedUp,
    /// 处理失败，可重试
    RetryableFailed,
}

/// 适配器发现的候选文件
///
/// 由扫描器或微信/文件夹适配器遍历时产出，传递给后续稳定性检测和入库逻辑。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredFile {
    /// 来源类型标签（如 "wechat-windows", "generic-folder"）
    pub source_type: String,
    /// 归属账号（如微信账号）
    pub source_account_id: Option<String>,
    /// 文件绝对路径
    pub absolute_path: String,
    /// 原始文件名
    pub file_name: String,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 文件修改时间
    pub modified_time: DateTime<Utc>,
    /// 来源系统提供的稳定会话 ID（若无法可靠提取则为 None）
    pub source_conversation_id: Option<String>,
}

/// 同步日志事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalEventType {
    /// 新增文件来源记录（含对象与本机路径可选信息）
    FileRecordAdded,
    /// 删除文件来源记录（软删除 tombstone）
    FileRecordDeleted,
    /// 用户更新来源账号名称或收藏状态
    SourceAccountUpdated,
    /// 用户更新来源聊天名称或收藏状态
    SourceConversationUpdated,
    /// 内容对象隐藏（软隐藏，可恢复）
    ObjectHidden,
    /// 内容对象恢复可见
    ObjectRestored,
    /// 内容对象彻底删除（须先隐藏；压过 hide/restore）
    ObjectPurged,
}

/// 元数据日志事件
///
/// 每个设备在自己的 epoch 下维护连续 seq；事件不可变，重放幂等。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEvent {
    pub event_id: String,
    pub device_id: String,
    pub epoch: u64,
    pub seq: u64,
    pub logical_clock: u64,
    pub schema_version: u32,
    pub event_type: JournalEventType,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 设备同步游标
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncCursor {
    pub device_id: String,
    pub epoch: u64,
    pub last_contiguous_seq: u64,
}

/// 一次流水线/恢复运行的类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunKind {
    /// 扫描 → 归档 → 同步
    Pipeline,
    /// 从远端恢复索引
    Restore,
}

impl TaskRunKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskRunKind::Pipeline => "pipeline",
            TaskRunKind::Restore => "restore",
        }
    }
}

/// 运行整体结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunStatus {
    /// 执行中
    Running,
    /// 全部阶段成功且无失败明细
    Success,
    /// 流水线跑完但存在失败/跳过项
    Partial,
    /// 阶段中断或应用异常退出
    Failed,
    /// 用户请求取消且协作点已停止
    Cancelled,
}

impl TaskRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskRunStatus::Running => "running",
            TaskRunStatus::Success => "success",
            TaskRunStatus::Partial => "partial",
            TaskRunStatus::Failed => "failed",
            TaskRunStatus::Cancelled => "cancelled",
        }
    }
}

/// 运行阶段标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunStageName {
    Scan,
    Archive,
    Publish,
    Pull,
}

impl TaskRunStageName {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskRunStageName::Scan => "scan",
            TaskRunStageName::Archive => "archive",
            TaskRunStageName::Publish => "publish",
            TaskRunStageName::Pull => "pull",
        }
    }
}

/// 阶段执行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunStageStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
}

impl TaskRunStageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskRunStageStatus::Pending => "pending",
            TaskRunStageStatus::Running => "running",
            TaskRunStageStatus::Success => "success",
            TaskRunStageStatus::Failed => "failed",
            TaskRunStageStatus::Skipped => "skipped",
        }
    }
}

/// 运行明细项状态（仅关键项入库）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunItemStatus {
    /// 上传/处理失败（可重试）
    Failed,
    /// 本地文件缺失
    Missing,
    /// 主动跳过（暂停等）
    Skipped,
}

impl TaskRunItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskRunItemStatus::Failed => "failed",
            TaskRunItemStatus::Missing => "missing",
            TaskRunItemStatus::Skipped => "skipped",
        }
    }
}

/// 一次运行记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRun {
    pub run_id: String,
    pub kind: TaskRunKind,
    pub trigger_source: String,
    pub status: TaskRunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub webdav_configured: bool,
    pub summary_json: Option<String>,
    pub error_message: Option<String>,
    pub runner_kind: String,
    pub runner_pid: Option<i64>,
    pub cancel_requested: bool,
    pub heartbeat_at: Option<DateTime<Utc>>,
}

/// 运行内阶段记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRunStage {
    pub stage_id: String,
    pub run_id: String,
    pub stage: TaskRunStageName,
    pub status: TaskRunStageStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub stats_json: Option<String>,
    pub message: Option<String>,
}

/// 运行文件级关键明细
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRunItem {
    pub item_id: String,
    pub run_id: String,
    pub stage: TaskRunStageName,
    pub record_id: Option<String>,
    pub object_id: Option<String>,
    pub task_id: Option<String>,
    pub name: String,
    pub status: TaskRunItemStatus,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub size: Option<i64>,
    pub updated_at: DateTime<Utc>,
}

/// 远端设备注册信息
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub display_name: Option<String>,
    pub epoch: u64,
    pub last_seq: u64,
    pub updated_at: DateTime<Utc>,
}
