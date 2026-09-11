//! # 核心错误定义模块
//!
//! 提供 ChatVault 系统的统一错误类型与 Result 别名，便于在不同模块间传递领域错误。

use thiserror::Error;

/// ChatVault 系统的基础错误类型
#[derive(Debug, Error)]
pub enum ChatVaultError {
    /// IO 操作异常
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 序列化或反序列化失败
    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    /// 本地数据库操作错误
    #[error("数据库操作错误: {0}")]
    Database(String),

    /// WebDAV 远端网络与协议交互错误
    #[error("WebDAV 通信错误: {0}")]
    WebDav(String),

    /// 哈希校验不匹配
    #[error("文件哈希校验失败: 预期 {expected}, 实际 {actual}")]
    HashMismatch { expected: String, actual: String },

    /// 文件不可用或已被移除
    #[error("文件未找到或不可读: {path}")]
    FileNotFound { path: String },

    /// 微信目录解析失败
    #[error("微信目录解析失败: {0}")]
    WeChatParse(String),

    /// 扫描器操作错误
    #[error("扫描错误: {0}")]
    Scan(String),

    /// 通用未知错误
    #[error("未知内部错误: {0}")]
    Internal(String),
}

/// 统一的 Result 别名
pub type Result<T> = std::result::Result<T, ChatVaultError>;
