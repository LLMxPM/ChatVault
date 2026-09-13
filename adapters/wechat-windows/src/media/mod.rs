//! # 微信 4.x 聊天图片离线解密子模块
//!
//! 承载微信专属逻辑：白名单参数文件名解析、任务内密钥派生、V2 三段结构
//! 解密、标准图片完整校验、变体识别与 `conv_hash` 会话路径解析。
//! 密钥与 code 仅在任务内存中使用，不写入任何持久化介质。

pub mod images;
pub mod parameters;
pub mod prepare;
pub mod v2;

pub use images::{
    conv_hash_for_path, discover_image_candidates, normalize_stem, variant_hint_from_filename,
    ImageCandidate, MediaVariant,
};
pub use parameters::{
    default_kvcomm_dirs, derive_key_material, normalize_account_id, prepare_account_candidates,
    scan_parameter_codes, AccountKeyMaterial, ParameterScanError,
};
pub use prepare::{
    count_parameter_codes, parameter_directories, prepare_account_images, try_decrypt_image,
    ImagePrepBatch, ImagePrepFailure, ImagePrepStats, PreparedImage,
};
pub use v2::{decrypt_v2, looks_like_v2, parse_v2_header, V2DecryptError, V2Header};
