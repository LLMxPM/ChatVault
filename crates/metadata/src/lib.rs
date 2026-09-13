//! # ChatVault Metadata 库
//!
//! 提供 BLAKE3 流式哈希计算、对象路径规则及格式化方法。

pub mod format;
pub mod hasher;
pub mod staging;
pub mod validation;
pub use validation::{
    format_vault_id, validate_hash, validate_id, validate_vault_id, vault_id_suffix,
    VAULT_ID_PREFIX,
};

pub use format::*;
pub use hasher::*;
