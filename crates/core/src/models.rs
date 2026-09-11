//! # 核心领域模型定义
//!
//! 定义 ChatVault 中的基础实体，包括资料库配置、内容对象、文件来源记录、
//! 本地文件关联、状态流转枚举以及由适配器发掘出的待处理文件。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    /// 输出: 具有随机 UUID 和 blake3 算法的 VaultConfig
    fn default() -> Self {
        Self {
            vault_id: uuid::Uuid::new_v4().to_string(),
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
    /// 发现来源类型（例如 wechat-windows, generic-folder）
    pub source: String,
    /// 来源账号 ID（例如微信 wxid_xxx 或微信号）
    pub account_id: Option<String>,
    /// 会话标识（若可提取，否则为 None）
    pub conversation_id: Option<String>,
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
    pub account_id: Option<String>,
    /// 文件绝对路径
    pub absolute_path: String,
    /// 原始文件名
    pub file_name: String,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 文件修改时间
    pub modified_time: DateTime<Utc>,
    /// 推测或提取到的会话上下文（若无则为 None）
    pub conversation_hint: Option<String>,
}
