//! # ChatVault WebDAV 库
//!
//! 封装 WebDAV 客户端请求、能力探测、远端回读哈希校验与 Vault 配置。

pub mod capability;
pub mod client;
mod immutable;
mod propfind;
pub mod publisher;
pub mod vault;
pub mod verify;

pub use capability::{CapabilityDetector, CapabilityReport};
pub use client::{WebDavClient, WebDavConfig};
pub use publisher::{ObjectPublisher, PublishResult};
pub use vault::{ensure_vault_config, load_vault_config};
pub use verify::RemoteVerifier;

/// 系统凭据键：与 storage_identity 同源的 URL 规范化 + 用户名。
///
/// 读、写、删必须共用本函数，避免大小写/末尾斜杠不一致导致清错或清不掉。
pub fn credential_key(url: &str, username: &str) -> String {
    let normalized = reqwest::Url::parse(url.trim())
        .map(|u| u.to_string().trim_end_matches('/').to_string())
        .unwrap_or_else(|_| url.trim().trim_end_matches('/').to_string());
    format!("{normalized}:{}", username.trim())
}
