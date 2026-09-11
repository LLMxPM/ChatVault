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
