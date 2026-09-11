//! # ChatVault Metadata 库
//!
//! 提供 BLAKE3 流式哈希计算、对象路径规则及格式化方法。

pub mod format;
pub mod hasher;
pub mod staging;
pub mod validation;
pub use validation::{validate_hash, validate_id};

pub use format::*;
pub use hasher::*;
