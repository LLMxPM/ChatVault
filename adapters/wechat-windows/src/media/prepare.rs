//! # 图片准备编排：发现 → 稳定 → 解密 → 校验 → 明文暂存
//!
//! 桌面与 CLI 共用；微信准备细节调用适配器 API，不感知 V2 内部结构。

use crate::media::{
    conv_hash_for_path, decrypt_v2, default_kvcomm_dirs, derive_key_material,
    discover_image_candidates, looks_like_v2, normalize_account_id, normalize_stem,
    prepare_account_candidates, scan_parameter_codes, ImageCandidate, MediaVariant, V2DecryptError,
};
use chatvault_core::models::{ImageErrorCode, PreparedContent};
use chatvault_metadata::{looks_like_standard_image, validate_image_bytes};
use chatvault_scanner::check_file_stability_sync;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 单个账号的图片准备结果统计
#[derive(Debug, Default, Clone)]
pub struct ImagePrepStats {
    /// 已发现候选数
    pub discovered: usize,
    /// 解密校验成功并暂存数
    pub prepared: usize,
    /// 参数不可用
    pub parameters_unavailable: usize,
    /// 参数不适用（有 code 但未命中）
    pub parameters_not_applicable: usize,
    /// 非 V2 / 结构非法
    pub unsupported_structure: usize,
    /// WXGF 等未验收容器
    pub unsupported_payload: usize,
    /// 完整解码失败
    pub invalid_image: usize,
    /// 等待稳定
    pub waiting_stable: usize,
    /// 逻辑图片组数（去重后）
    pub logical_groups: usize,
}

/// 图片准备产物
#[derive(Debug)]
pub struct PreparedImage {
    /// 对应的候选信息
    pub candidate: ImageCandidate,
    /// 已准备的明文内容
    pub prepared: PreparedContent,
    /// 明文暂存路径
    pub plaintext_path: PathBuf,
}

/// 图片准备失败信息（不含密钥）
#[derive(Debug)]
pub struct ImagePrepFailure {
    pub candidate: ImageCandidate,
    pub error_code: ImageErrorCode,
}

/// 图片准备批次结果
pub struct ImagePrepBatch {
    pub prepared: Vec<PreparedImage>,
    pub failures: Vec<ImagePrepFailure>,
    pub stats: ImagePrepStats,
}

/// 准备一个账号的全部可处理图片。
///
/// 输入: 图片根目录、账号目录名、受控明文暂存目录。
/// 输出: 成功准备的明文列表与失败明细。
pub fn prepare_account_images(
    images_dir: &Path,
    account_dir_name: &str,
    staging_dir: &Path,
    max_candidates: usize,
) -> ImagePrepBatch {
    let mut batch = ImagePrepBatch {
        prepared: Vec::new(),
        failures: Vec::new(),
        stats: ImagePrepStats::default(),
    };

    let candidates = discover_image_candidates(images_dir, account_dir_name);
    batch.stats.discovered = candidates.len();
    if candidates.is_empty() {
        return batch;
    }

    // 预分组统计
    let groups: std::collections::HashSet<_> = candidates
        .iter()
        .map(|c| c.image_group_key.clone())
        .collect();
    batch.stats.logical_groups = groups.len();

    // 扫描参数
    let key_materials = match prepare_account_candidates(account_dir_name, None, max_candidates) {
        Ok(m) => m,
        Err(_) => {
            // 所有候选标记为参数不可用
            batch.stats.parameters_unavailable = candidates.len();
            for c in candidates {
                batch.failures.push(ImagePrepFailure {
                    candidate: c,
                    error_code: ImageErrorCode::MediaParametersUnavailable,
                });
            }
            return batch;
        }
    };

    if key_materials.is_empty() {
        batch.stats.parameters_unavailable = candidates.len();
        for c in candidates {
            batch.failures.push(ImagePrepFailure {
                candidate: c,
                error_code: ImageErrorCode::MediaParametersUnavailable,
            });
        }
        return batch;
    }

    // 组内最大像素表（用于变体判定）
    let mut group_max_pixels: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();

    // 第一遍：尝试全部候选，收集尺寸
    let mut decrypted_ok: Vec<(ImageCandidate, Vec<u8>, MediaVariant)> = Vec::new();

    for candidate in candidates {
        // 稳定性检查
        let stable = check_file_stability_sync(&candidate.source_path, Duration::from_millis(50))
            .unwrap_or(false);
        if !stable {
            batch.stats.waiting_stable += 1;
            batch.failures.push(ImagePrepFailure {
                candidate,
                error_code: ImageErrorCode::WaitingStable,
            });
            continue;
        }

        let data = match std::fs::read(&candidate.source_path) {
            Ok(d) => d,
            Err(_) => {
                batch.failures.push(ImagePrepFailure {
                    candidate,
                    error_code: ImageErrorCode::SourceChanged,
                });
                continue;
            }
        };

        // 标准明文图片（另存到通用目录的情况不在此路径）
        if looks_like_standard_image(&data) {
            match validate_image_bytes(&data) {
                Ok(validated) => {
                    let pixels = validated.width as u64 * validated.height as u64;
                    let entry = group_max_pixels
                        .entry(candidate.image_group_key.clone())
                        .or_insert((0, 0));
                    if pixels > entry.0 as u64 * entry.1 as u64 {
                        *entry = (validated.width, validated.height);
                    }
                    let variant = MediaVariant::Display;
                    decrypted_ok.push((candidate, data, variant));
                }
                Err(_) => {
                    batch.stats.invalid_image += 1;
                    batch.failures.push(ImagePrepFailure {
                        candidate,
                        error_code: ImageErrorCode::InvalidImage,
                    });
                }
            }
            continue;
        }

        // V2 加密图片
        if !looks_like_v2(&data) {
            batch.stats.unsupported_structure += 1;
            batch.failures.push(ImagePrepFailure {
                candidate,
                error_code: ImageErrorCode::UnsupportedStructure,
            });
            continue;
        }

        // 尝试所有密钥候选
        let mut decrypted: Option<Vec<u8>> = None;
        let mut any_structure_ok = false;
        for material in &key_materials {
            match decrypt_v2(&data, &material.aes_key, material.xor_byte) {
                Ok(plain) => {
                    decrypted = Some(plain);
                    break;
                }
                Err(V2DecryptError::SignatureMismatch) => continue,
                Err(V2DecryptError::TooShort) => {
                    any_structure_ok = true;
                    continue;
                }
                Err(_) => {
                    any_structure_ok = true;
                    continue;
                }
            }
        }

        let Some(plain) = decrypted else {
            if any_structure_ok {
                // 头合法但密钥未命中
                batch.stats.parameters_not_applicable += 1;
                batch.failures.push(ImagePrepFailure {
                    candidate,
                    error_code: ImageErrorCode::AccountParametersMiss,
                });
            } else {
                batch.stats.unsupported_structure += 1;
                batch.failures.push(ImagePrepFailure {
                    candidate,
                    error_code: ImageErrorCode::UnsupportedStructure,
                });
            }
            continue;
        };

        // 完整校验
        match validate_image_bytes(&plain) {
            Ok(validated) => {
                let pixels = validated.width as u64 * validated.height as u64;
                let entry = group_max_pixels
                    .entry(candidate.image_group_key.clone())
                    .or_insert((0, 0));
                if pixels > entry.0 as u64 * entry.1 as u64 {
                    *entry = (validated.width, validated.height);
                }
                let hint = candidate.filename_variant;
                // 暂存为 Unknown，第二遍根据组内最大尺寸修正
                decrypted_ok.push((candidate, plain, hint.unwrap_or(MediaVariant::Unknown)));
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("unsupported_payload") {
                    batch.stats.unsupported_payload += 1;
                    batch.failures.push(ImagePrepFailure {
                        candidate,
                        error_code: ImageErrorCode::UnsupportedPayload,
                    });
                } else {
                    batch.stats.invalid_image += 1;
                    batch.failures.push(ImagePrepFailure {
                        candidate,
                        error_code: ImageErrorCode::InvalidImage,
                    });
                }
            }
        }
    }

    // 第二遍：写入明文暂存并构建 PreparedContent
    let _ = std::fs::create_dir_all(staging_dir);
    for (candidate, plain, hint_variant) in decrypted_ok {
        let validated = match validate_image_bytes(&plain) {
            Ok(v) => v,
            Err(_) => {
                batch.stats.invalid_image += 1;
                batch.failures.push(ImagePrepFailure {
                    candidate,
                    error_code: ImageErrorCode::InvalidImage,
                });
                continue;
            }
        };

        let (max_w, max_h) = group_max_pixels
            .get(&candidate.image_group_key)
            .copied()
            .unwrap_or((validated.width, validated.height));
        let variant = crate::media::images::resolve_variant(
            Some(hint_variant),
            validated.width,
            validated.height,
            max_w as u64 * max_h as u64,
        );

        let hash = chatvault_metadata::compute_blake3_bytes(&plain);
        let export_name = format!("{}.{}", candidate.normalized_stem, validated.extension);
        let plaintext_path = staging_dir.join(&hash.hex_hash);

        // 原子写入明文
        if !plaintext_path.exists() {
            if let Err(e) = std::fs::write(&plaintext_path, &plain) {
                tracing::warn!("写入明文暂存失败 {}: {}", plaintext_path.display(), e);
                batch.failures.push(ImagePrepFailure {
                    candidate,
                    error_code: ImageErrorCode::InsufficientSpace,
                });
                continue;
            }
        }

        let prepared = PreparedContent {
            plaintext_path: plaintext_path.to_string_lossy().to_string(),
            content_hash: hash.hex_hash.clone(),
            size: validated.size,
            mime: validated.mime.clone(),
            extension: validated.extension.clone(),
            width: Some(validated.width),
            height: Some(validated.height),
            frame_count: Some(validated.frame_count),
            source_type: chatvault_core::models::WECHAT_WINDOWS_4_SOURCE_TYPE.to_string(),
            source_account_id: Some(account_dir_name.to_string()),
            source_conversation_id: candidate.conv_hash.clone(),
            export_name,
            source_original_name: candidate.file_name.clone(),
            source_path: candidate.source_path.to_string_lossy().to_string(),
            source_mtime_ms: candidate.modified_time.timestamp_millis(),
            source_size: candidate.file_size,
            media_variant: Some(variant.as_str().to_string()),
            image_group_key: Some(candidate.image_group_key.clone()),
            file_time: candidate.modified_time,
        };

        batch.stats.prepared += 1;
        batch.prepared.push(PreparedImage {
            candidate,
            prepared,
            plaintext_path,
        });
    }

    batch
}

/// 尝试解密单个文件（供测试与调试用）
pub fn try_decrypt_image(
    data: &[u8],
    codes: &[u32],
    account_dir_name: &str,
) -> std::result::Result<Vec<u8>, ImageErrorCode> {
    if !looks_like_v2(data) {
        return Err(ImageErrorCode::UnsupportedStructure);
    }
    let materials: Vec<_> = codes
        .iter()
        .map(|&c| derive_key_material(c, account_dir_name))
        .collect();
    for m in &materials {
        match decrypt_v2(data, &m.aes_key, m.xor_byte) {
            Ok(plain) => return Ok(plain),
            Err(V2DecryptError::SignatureMismatch) => continue,
            Err(_) => continue,
        }
    }
    Err(ImageErrorCode::AccountParametersMiss)
}

/// 获取默认参数目录（供 UI 展示说明）
pub fn parameter_directories() -> Vec<PathBuf> {
    default_kvcomm_dirs()
}

/// 扫描默认参数目录的 code 数量（供诊断，不暴露具体 code）
pub fn count_parameter_codes() -> Option<usize> {
    let dirs = default_kvcomm_dirs();
    scan_parameter_codes(&dirs).ok().map(|c| c.len())
}

/// 规范化账号标识（公开函数，供编排层使用）
pub fn account_identity(account_dir_name: &str) -> String {
    normalize_account_id(account_dir_name)
}

/// 从路径解析 conv_hash（公开函数）
pub fn parse_conv_hash(images_root: &Path, file_path: &Path) -> Option<String> {
    conv_hash_for_path(images_root, file_path)
}

/// 规范化 stem（公开函数）
pub fn public_normalize_stem(stem: &str) -> String {
    normalize_stem(stem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncrypt, KeyInit};
    use aes::Aes128;

    #[allow(clippy::manual_repeat_n, clippy::chunks_exact_to_as_chunks)]
    fn build_v2(
        aes_plain: &[u8],
        raw_mid: &[u8],
        xor_plain: &[u8],
        key: &[u8; 16],
        xor_byte: u8,
    ) -> Vec<u8> {
        let pad = 16 - (aes_plain.len() % 16);
        let mut padded = aes_plain.to_vec();
        padded.extend(std::iter::repeat(pad as u8).take(pad));
        let cipher = Aes128::new(key.into());
        let mut encrypted = padded.clone();
        for chunk in encrypted.chunks_exact_mut(16) {
            cipher.encrypt_block(chunk.into());
        }
        let xor_enc: Vec<u8> = xor_plain.iter().map(|b| b ^ xor_byte).collect();
        let mut out = Vec::new();
        out.extend_from_slice(&crate::media::v2::V2_SIGNATURE);
        out.extend_from_slice(&(aes_plain.len() as u32).to_le_bytes());
        out.extend_from_slice(&(xor_enc.len() as u32).to_le_bytes());
        out.push(1u8);
        out.extend_from_slice(&encrypted);
        out.extend_from_slice(raw_mid);
        out.extend_from_slice(&xor_enc);
        out
    }

    fn minimal_png() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([10, 20, 30]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn decrypts_synthetic_v2_png() {
        let png = minimal_png();
        let code = 999999u32;
        let material = derive_key_material(code, "wxid_test_0001");
        // 将 PNG 拆成 AES 段 + 原样段
        let split = png.len() / 2;
        let aes_part = &png[..split];
        let raw_part = &png[split..];
        let cipher = build_v2(
            aes_part,
            raw_part,
            b"",
            &material.aes_key,
            material.xor_byte,
        );

        let result = try_decrypt_image(&cipher, &[code], "wxid_test_0001").unwrap();
        assert_eq!(result, png);
    }

    #[test]
    fn wrong_code_reports_miss() {
        let png = minimal_png();
        let code = 999999u32;
        let material = derive_key_material(code, "wxid_test_0001");
        let split = png.len() / 2;
        let cipher = build_v2(
            &png[..split],
            &png[split..],
            b"",
            &material.aes_key,
            material.xor_byte,
        );
        let result = try_decrypt_image(&cipher, &[12345], "wxid_test_0001");
        assert_eq!(result, Err(ImageErrorCode::AccountParametersMiss));
    }

    #[test]
    fn non_v2_reports_unsupported_structure() {
        let result = try_decrypt_image(b"not v2 at all here!!", &[1], "acc");
        assert_eq!(result, Err(ImageErrorCode::UnsupportedStructure));
    }
}
