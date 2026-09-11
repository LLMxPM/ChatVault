//! # ChatVault WebDAV 库
//!
//! 封装 WebDAV 客户端请求、能力探测与远端回读哈希校验。

pub mod capability;
pub mod client;
pub mod verify;

pub use capability::{CapabilityDetector, CapabilityReport};
pub use client::{WebDavConfig, WebDavClient};
pub use verify::RemoteVerifier;
