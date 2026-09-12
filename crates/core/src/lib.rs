//! # ChatVault Core 库
//!
//! 提供领域实体定义与核心异常类型，供各子 crate 统一引用。

pub mod error;
pub mod models;
pub mod paths;

pub use error::{ChatVaultError, Result};
pub use models::*;
pub use paths::{is_under_root, normalize_root_path, normalize_scan_key};
