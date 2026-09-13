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
}

#[cfg(test)]
mod tests {
    use super::{CollectSource, GENERIC_FOLDER_SOURCE_TYPE};

    #[test]
    fn collect_source_uses_camel_case_wire_fields() {
        let source = CollectSource {
            source_type: GENERIC_FOLDER_SOURCE_TYPE.to_string(),
            path: r"C:\attachments".to_string(),
        };
        let encoded = serde_json::to_string(&source).unwrap();
        assert_eq!(
            encoded,
            r#"{"sourceType":"generic-folder","path":"C:\\attachments"}"#
        );
        assert_eq!(
            serde_json::from_str::<CollectSource>(&encoded).unwrap(),
            source
        );
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
    /// 图片变体类型（display/high/thumbnail/unknown）；非图片为 None
    pub media_variant: Option<String>,
    /// 同逻辑图片分组键；非图片为 None
    pub image_group_key: Option<String>,
    /// 源文件原始名（图片为 .dat 名）；与 original_name 可能不同
    pub source_original_name: Option<String>,
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
    /// 内容来源：original（明文源）或 decrypted（解密产物）
    pub content_origin: Option<String>,
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

/// 已准备好的明文内容（图片解密产物）
///
/// 携带受控明文路径、内容哈希/大小/类型、源状态及来源上下文；
/// 由 `ingest_prepared_content` 验证后事务入库。
#[derive(Debug, Clone)]
pub struct PreparedContent {
    /// 受控明文暂存路径
    pub plaintext_path: String,
    /// 明文 BLAKE3 hex
    pub content_hash: String,
    /// 明文大小（字节）
    pub size: u64,
    /// 真实 MIME
    pub mime: String,
    /// 建议扩展名（小写，不含点）
    pub extension: String,
    /// 图片宽（非图片为 None）
    pub width: Option<u32>,
    /// 图片高（非图片为 None）
    pub height: Option<u32>,
    /// 帧数（非图片为 None）
    pub frame_count: Option<u32>,
    /// 来源类型
    pub source_type: String,
    /// 来源账号 ID
    pub source_account_id: Option<String>,
    /// 来源会话 ID（图片为 conv_hash）
    pub source_conversation_id: Option<String>,
    /// 导出用文件名
    pub export_name: String,
    /// 源文件原始名（.dat）
    pub source_original_name: String,
    /// 源文件绝对路径
    pub source_path: String,
    /// 源 mtime 毫秒
    pub source_mtime_ms: i64,
    /// 源大小
    pub source_size: u64,
    /// 图片变体类型
    pub media_variant: Option<String>,
    /// 图片分组键
    pub image_group_key: Option<String>,
    /// 文件时间
    pub file_time: DateTime<Utc>,
}

/// 图片候选处理状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageCandidateStatus {
    /// 已发现
    Discovered,
    /// 等待源文件稳定
    WaitingStable,
    /// 准备中
    Preparing,
    /// 解密完成待校验
    DecryptedPendingVerify,
    /// 明文已校验
    VerifiedPlaintext,
    /// 已入库
    Ingested,
    /// 已归档
    Archived,
    /// 失败（见错误码）
    Failed,
}

impl ImageCandidateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImageCandidateStatus::Discovered => "discovered",
            ImageCandidateStatus::WaitingStable => "waiting_stable",
            ImageCandidateStatus::Preparing => "preparing",
            ImageCandidateStatus::DecryptedPendingVerify => "decrypted_pending_verify",
            ImageCandidateStatus::VerifiedPlaintext => "verified_plaintext",
            ImageCandidateStatus::Ingested => "ingested",
            ImageCandidateStatus::Archived => "archived",
            ImageCandidateStatus::Failed => "failed",
        }
    }
}

/// 图片处理错误码（与规划文档 §6.2 对应）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageErrorCode {
    /// 写入中或大小/mtime 不稳定
    WaitingStable,
    /// 0 字节
    EmptySource,
    /// 准备期间源被改写
    SourceChanged,
    /// 白名单内无任何 code
    MediaParametersUnavailable,
    /// 账号规范化无法确定
    AccountIdentityUnconfirmed,
    /// 本轮有限候选均未命中该源
    ParametersNotApplicable,
    /// code/候选数超上限
    CandidateLimitExceeded,
    /// 有 code 但对应该账号/该文件不适用
    AccountParametersMiss,
    /// 非 V2 / 未知标志 / 边界非法
    UnsupportedStructure,
    /// 解密后为 WXGF 等未验收容器
    UnsupportedPayload,
    /// 头合法但完整解码失败
    InvalidImage,
    /// 磁盘不足
    InsufficientSpace,
    /// 明文缓存丢失
    PreparedContentMissing,
}

impl ImageErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImageErrorCode::WaitingStable => "waiting_stable",
            ImageErrorCode::EmptySource => "empty_source",
            ImageErrorCode::SourceChanged => "source_changed",
            ImageErrorCode::MediaParametersUnavailable => "media_parameters_unavailable",
            ImageErrorCode::AccountIdentityUnconfirmed => "account_identity_unconfirmed",
            ImageErrorCode::ParametersNotApplicable => "parameters_not_applicable",
            ImageErrorCode::CandidateLimitExceeded => "candidate_limit_exceeded",
            ImageErrorCode::AccountParametersMiss => "account_parameters_miss",
            ImageErrorCode::UnsupportedStructure => "unsupported_structure",
            ImageErrorCode::UnsupportedPayload => "unsupported_payload",
            ImageErrorCode::InvalidImage => "invalid_image",
            ImageErrorCode::InsufficientSpace => "insufficient_space",
            ImageErrorCode::PreparedContentMissing => "prepared_content_missing",
        }
    }

    /// 是否默认自动重试
    pub fn auto_retry(&self) -> bool {
        matches!(
            self,
            ImageErrorCode::WaitingStable
                | ImageErrorCode::EmptySource
                | ImageErrorCode::SourceChanged
                | ImageErrorCode::MediaParametersUnavailable
                | ImageErrorCode::ParametersNotApplicable
                | ImageErrorCode::CandidateLimitExceeded
                | ImageErrorCode::AccountParametersMiss
                | ImageErrorCode::InsufficientSpace
                | ImageErrorCode::PreparedContentMissing
        )
    }
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

/// 远端设备注册信息
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub display_name: Option<String>,
    pub epoch: u64,
    pub last_seq: u64,
    pub updated_at: DateTime<Utc>,
}
