//! # ChatVault Metadata 库
//!
//! 提供 BLAKE3 流式哈希计算、对象路径规则、完整图片校验及格式化方法。

pub mod format;
pub mod hasher;
pub mod image_validation;
pub mod staging;
pub mod validation;
pub use validation::{
    format_vault_id, validate_hash, validate_id, validate_vault_id, vault_id_suffix,
    VAULT_ID_PREFIX,
};

pub use format::*;
pub use hasher::*;
pub use image_validation::{
    looks_like_standard_image, looks_like_wxgf, mime_from_extension, validate_image_bytes,
    validate_image_file, ValidatedImage,
};
