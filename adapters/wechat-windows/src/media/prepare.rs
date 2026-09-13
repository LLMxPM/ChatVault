//! # 图片准备编排：发现 → 稳定 → 解密 → 校验 → 明文暂存
//!
//! 桌面与 CLI 共用；微信准备细节调用适配器 API，不感知 V2 内部结构。

use crate::media::{
    conv_hash_for_path, decrypt_v2, default_kvcomm_dirs, derive_key_material, looks_like_v2,
    normalize_account_id, normalize_stem, prepare_account_candidates, scan_parameter_codes,
    ImageCandidate, MediaVariant,
};
use chatvault_core::models::{ImageErrorCode, PreparedContent};
use chatvault_metadata::{looks_like_standard_image, validate_image_bytes};
use chatvault_scanner::check_file_stability_sync;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 单文件解密读取上限，防止异常源文件耗尽桌面进程内存。
const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;

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

/// 准备已由索引层筛选出的图片候选。
///
/// 每次只把一个源文件读入内存；标准图片不依赖微信参数，V2 图片只有在
/// 完整解密并通过标准解码后才写入受控 pending 目录。
pub fn prepare_image_candidates(
    candidates: Vec<ImageCandidate>,
    account_dir_name: &str,
    staging_dir: &Path,
    max_candidates: usize,
) -> ImagePrepBatch {
    let mut batch = ImagePrepBatch {
        prepared: Vec::new(),
        failures: Vec::new(),
        stats: ImagePrepStats::default(),
    };

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

    // 组内最大像素表（用于变体判定）
    let mut group_max_pixels: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();

    // 参数只在遇到第一张 V2 图片时读取；标准明文图片无需参数。
    // 失败态也缓存，避免对每张 V2 图重复扫描 kvcomm。
    enum ParamState {
        Ready(Vec<crate::media::AccountKeyMaterial>),
        Unavailable,
        LimitExceeded,
    }
    let mut param_state: Option<ParamState> = None;

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

        let (data, source_size, source_mtime_ms, source_digest) =
            match read_source_stable(&candidate.source_path) {
                Ok(value) => value,
                Err(error_code) => {
                    batch.failures.push(ImagePrepFailure {
                        candidate,
                        error_code,
                    });
                    continue;
                }
            };
        if source_size != candidate.file_size
            || source_mtime_ms != candidate.modified_time.timestamp_millis()
        {
            batch.failures.push(ImagePrepFailure {
                candidate,
                error_code: ImageErrorCode::SourceChanged,
            });
            continue;
        }

        // 标准明文图片（另存到通用目录的情况不在此路径）
        if looks_like_standard_image(&data) {
            match validate_image_bytes(&data) {
                Ok(validated) => {
                    if let Some(item) = build_prepared_image(
                        candidate.clone(),
                        data,
                        validated,
                        MediaVariant::Display,
                        account_dir_name,
                        staging_dir,
                        source_size,
                        source_mtime_ms,
                        source_digest,
                    ) {
                        update_group_max(&mut group_max_pixels, &item.prepared);
                        batch.prepared.push(item);
                    } else {
                        batch.failures.push(ImagePrepFailure {
                            candidate,
                            error_code: ImageErrorCode::InsufficientSpace,
                        });
                    }
                }
                Err(error) => {
                    let error_code = if error.to_string().contains("unsupported_payload") {
                        batch.stats.unsupported_payload += 1;
                        ImageErrorCode::UnsupportedPayload
                    } else {
                        batch.stats.invalid_image += 1;
                        ImageErrorCode::InvalidImage
                    };
                    batch.failures.push(ImagePrepFailure {
                        candidate,
                        error_code,
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

        if param_state.is_none() {
            match prepare_account_candidates(account_dir_name, None, max_candidates) {
                Ok(materials) if !materials.is_empty() => {
                    param_state = Some(ParamState::Ready(materials));
                }
                Ok(_) => param_state = Some(ParamState::Unavailable),
                Err(error) if error.to_string().contains("candidate_limit_exceeded") => {
                    param_state = Some(ParamState::LimitExceeded);
                }
                Err(_) => param_state = Some(ParamState::Unavailable),
            }
        }
        let materials = match &param_state {
            Some(ParamState::Ready(materials)) => materials,
            Some(ParamState::Unavailable) => {
                batch.stats.parameters_unavailable += 1;
                batch.failures.push(ImagePrepFailure {
                    candidate,
                    error_code: ImageErrorCode::MediaParametersUnavailable,
                });
                continue;
            }
            Some(ParamState::LimitExceeded) => {
                batch.failures.push(ImagePrepFailure {
                    candidate,
                    error_code: ImageErrorCode::CandidateLimitExceeded,
                });
                continue;
            }
            None => continue,
        };

        // 尝试所有密钥候选
        let mut prepared: Option<(Vec<u8>, chatvault_metadata::ValidatedImage)> = None;
        let mut any_decrypted = false;
        let mut any_unsupported_payload = false;
        // 只有完整图片校验成功才接受候选，填充成功但图片损坏时继续尝试。
        for material in materials {
            if let Ok(plain) = decrypt_v2(&data, &material.aes_key, material.xor_byte) {
                any_decrypted = true;
                match validate_image_bytes(&plain) {
                    Ok(validated) => {
                        prepared = Some((plain, validated));
                        break;
                    }
                    Err(error) if error.to_string().contains("unsupported_payload") => {
                        any_unsupported_payload = true;
                    }
                    Err(_) => {}
                }
            }
        }

        let Some((plain, validated)) = prepared else {
            if any_unsupported_payload {
                batch.stats.unsupported_payload += 1;
                batch.failures.push(ImagePrepFailure {
                    candidate,
                    error_code: ImageErrorCode::UnsupportedPayload,
                });
            } else if any_decrypted {
                batch.stats.invalid_image += 1;
                batch.failures.push(ImagePrepFailure {
                    candidate,
                    error_code: ImageErrorCode::InvalidImage,
                });
            } else if crate::media::parse_v2_header(&data).is_ok() {
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

        let hint = candidate.filename_variant.unwrap_or(MediaVariant::Unknown);
        if let Some(item) = build_prepared_image(
            candidate.clone(),
            plain,
            validated,
            hint,
            account_dir_name,
            staging_dir,
            source_size,
            source_mtime_ms,
            source_digest,
        ) {
            update_group_max(&mut group_max_pixels, &item.prepared);
            batch.prepared.push(item);
        } else {
            batch.failures.push(ImagePrepFailure {
                candidate,
                error_code: ImageErrorCode::InsufficientSpace,
            });
        }
    }

    // 第二遍只修正变体标签，不保留任何整图字节。
    for item in &mut batch.prepared {
        let (max_w, max_h) = group_max_pixels
            .get(item.prepared.image_group_key.as_deref().unwrap_or_default())
            .copied()
            .unwrap_or((
                item.prepared.width.unwrap_or(1),
                item.prepared.height.unwrap_or(1),
            ));
        let hint = item
            .prepared
            .media_variant
            .as_deref()
            .map(|value| match value {
                "display" => MediaVariant::Display,
                "high" => MediaVariant::High,
                "thumbnail" => MediaVariant::Thumbnail,
                _ => MediaVariant::Unknown,
            });
        let variant = crate::media::images::resolve_variant(
            hint,
            item.prepared.width.unwrap_or(1),
            item.prepared.height.unwrap_or(1),
            max_w as u64 * max_h as u64,
        );
        item.prepared.media_variant = Some(variant.as_str().to_string());
    }

    batch.stats.prepared = batch.prepared.len();
    batch
}

/// 读取并校验源文件快照，返回字节、大小、mtime 和源 BLAKE3 摘要。
fn read_source_stable(
    path: &Path,
) -> std::result::Result<(Vec<u8>, u64, i64, String), ImageErrorCode> {
    if crate::media::images::is_link_or_reparse(path) {
        return Err(ImageErrorCode::SourceChanged);
    }
    let before = std::fs::metadata(path).map_err(|_| ImageErrorCode::SourceChanged)?;
    if before.len() == 0 {
        return Err(ImageErrorCode::EmptySource);
    }
    if before.len() > MAX_SOURCE_BYTES {
        return Err(ImageErrorCode::InsufficientSpace);
    }
    let digest_before =
        chatvault_metadata::compute_blake3_file(path).map_err(|_| ImageErrorCode::SourceChanged)?;
    if digest_before.bytes_read != before.len() {
        return Err(ImageErrorCode::SourceChanged);
    }
    let mtime_before = before
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0);
    let file = std::fs::File::open(path).map_err(|_| ImageErrorCode::SourceChanged)?;
    let mut data = Vec::with_capacity(before.len() as usize);
    file.take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|_| ImageErrorCode::SourceChanged)?;
    if data.len() as u64 > MAX_SOURCE_BYTES {
        return Err(ImageErrorCode::InsufficientSpace);
    }
    let after = std::fs::metadata(path).map_err(|_| ImageErrorCode::SourceChanged)?;
    if crate::media::images::is_link_or_reparse(path) {
        return Err(ImageErrorCode::SourceChanged);
    }
    let mtime_after = after
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0);
    if before.len() != data.len() as u64
        || before.len() != after.len()
        || mtime_before != mtime_after
    {
        return Err(ImageErrorCode::SourceChanged);
    }
    let digest = chatvault_metadata::compute_blake3_bytes(&data).hex_hash;
    let digest_after =
        chatvault_metadata::compute_blake3_file(path).map_err(|_| ImageErrorCode::SourceChanged)?;
    if digest != digest_after.hex_hash || digest_before.hex_hash != digest_after.hex_hash {
        return Err(ImageErrorCode::SourceChanged);
    }
    Ok((data, after.len(), mtime_after, digest))
}

/// 将完整校验后的图片写入 pending 目录并构建入库上下文。
#[allow(clippy::too_many_arguments)]
fn build_prepared_image(
    candidate: ImageCandidate,
    data: Vec<u8>,
    validated: chatvault_metadata::ValidatedImage,
    hint: MediaVariant,
    account_dir_name: &str,
    staging_dir: &Path,
    source_size: u64,
    source_mtime_ms: i64,
    source_digest: String,
) -> Option<PreparedImage> {
    let pending_dir = staging_dir.join("pending");
    let plaintext_path =
        chatvault_metadata::staging::stage_bytes_pending(&data, &pending_dir).ok()?;
    let hash = chatvault_metadata::compute_blake3_bytes(&data);
    let stem = candidate.normalized_stem.trim();
    let export_name = if stem.is_empty() {
        format!("{}.{}", &hash.hex_hash[..12], validated.extension)
    } else {
        format!("{}.{}", stem, validated.extension)
    };
    let prepared = PreparedContent {
        plaintext_path: plaintext_path.to_string_lossy().to_string(),
        content_hash: hash.hex_hash,
        size: validated.size,
        mime: validated.mime,
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
        source_mtime_ms,
        source_size,
        source_digest: Some(source_digest),
        media_variant: Some(hint.as_str().to_string()),
        image_group_key: Some(candidate.image_group_key.clone()),
        file_time: candidate.modified_time,
    };
    Some(PreparedImage {
        candidate,
        prepared,
        plaintext_path,
    })
}

/// 更新会话分组的最大图片尺寸。
fn update_group_max(
    groups: &mut std::collections::HashMap<String, (u32, u32)>,
    prepared: &PreparedContent,
) {
    let Some(key) = prepared.image_group_key.as_deref() else {
        return;
    };
    let (width, height) = (prepared.width.unwrap_or(0), prepared.height.unwrap_or(0));
    let entry = groups.entry(key.to_string()).or_insert((0, 0));
    if u64::from(width) * u64::from(height) > u64::from(entry.0) * u64::from(entry.1) {
        *entry = (width, height);
    }
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
    if crate::media::parse_v2_header(data).is_err() {
        return Err(ImageErrorCode::UnsupportedStructure);
    }
    let materials: Vec<_> = codes
        .iter()
        .map(|&c| derive_key_material(c, account_dir_name))
        .collect();
    let mut any_decrypted = false;
    let mut any_unsupported_payload = false;
    for m in &materials {
        if let Ok(plain) = decrypt_v2(data, &m.aes_key, m.xor_byte) {
            any_decrypted = true;
            match validate_image_bytes(&plain) {
                Ok(_) => return Ok(plain),
                Err(error) if error.to_string().contains("unsupported_payload") => {
                    any_unsupported_payload = true;
                }
                Err(_) => {}
            }
        }
    }
    if any_unsupported_payload {
        Err(ImageErrorCode::UnsupportedPayload)
    } else if any_decrypted {
        Err(ImageErrorCode::InvalidImage)
    } else {
        Err(ImageErrorCode::AccountParametersMiss)
    }
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

    #[test]
    fn standard_png_prepares_without_parameter_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("cv_img_prepare_{unique}"));
        let image_dir = root.join("conv1").join("2026-09").join("Img");
        std::fs::create_dir_all(&image_dir).unwrap();
        let source = image_dir.join("plain.dat");
        std::fs::write(&source, minimal_png()).unwrap();
        let candidates = crate::media::discover_image_candidates(&root, "wxid_test", None);
        let staging = root.join("objects");
        let batch = prepare_image_candidates(candidates, "wxid_test", &staging, 64);
        assert_eq!(batch.prepared.len(), 1);
        assert!(batch.failures.is_empty());
        assert!(std::path::Path::new(&batch.prepared[0].prepared.plaintext_path).is_file());
        let _ = std::fs::remove_dir_all(root);
    }
}
