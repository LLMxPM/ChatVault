//! # 聊天图片候选发现、路径解析与变体识别
//!
//! 枚举 `msg/attach/<conv_hash>/<YYYY-MM>/Img/` 下的普通文件，
//! 解析 `conv_hash` 与月份，识别变体类型并计算分组键。

use chatvault_core::models::{DiscoveredFile, WECHAT_WINDOWS_4_SOURCE_TYPE};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 判断路径是否为符号链接或 Windows 重解析点；遍历微信目录时一律拒绝。
pub(crate) fn is_link_or_reparse(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return true;
    };
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// 将目录解析为真实路径，并确保仍位于允许的根目录内。
fn canonical_child(root: &Path, path: &Path) -> Option<PathBuf> {
    if is_link_or_reparse(path) {
        return None;
    }
    let canonical_root = std::fs::canonicalize(root).ok()?;
    let canonical_path = std::fs::canonicalize(path).ok()?;
    canonical_path
        .starts_with(&canonical_root)
        .then_some(canonical_path)
}

/// 图片变体类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaVariant {
    /// 显示图（默认展示）
    Display,
    /// 高清候选
    High,
    /// 缩略图
    Thumbnail,
    /// 无法区分
    Unknown,
}

impl MediaVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaVariant::Display => "display",
            MediaVariant::High => "high",
            MediaVariant::Thumbnail => "thumbnail",
            MediaVariant::Unknown => "unknown",
        }
    }

    /// 打开优先级：high > display > thumbnail > unknown
    pub fn open_priority(&self) -> u8 {
        match self {
            MediaVariant::High => 0,
            MediaVariant::Display => 1,
            MediaVariant::Thumbnail => 2,
            MediaVariant::Unknown => 3,
        }
    }
}

/// 单个图片候选文件
#[derive(Debug, Clone)]
pub struct ImageCandidate {
    /// 源密文绝对路径
    pub source_path: PathBuf,
    /// 源文件名
    pub file_name: String,
    /// 源文件大小
    pub file_size: u64,
    /// 源 mtime
    pub modified_time: DateTime<Utc>,
    /// 路径上的 `conv_hash`（会话不透明标识）
    pub conv_hash: Option<String>,
    /// 月份目录 `YYYY-MM`
    pub month: String,
    /// 规范化后的 stem（用于分组键）
    pub normalized_stem: String,
    /// 文件名启发式变体提示
    pub filename_variant: Option<MediaVariant>,
    /// 预分组键
    pub image_group_key: String,
}

/// 判断月份目录名是否为 `YYYY-MM`
fn is_month_directory(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 7
        && bytes[4] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || byte.is_ascii_digit())
        && matches!(name[5..7].parse::<u8>(), Ok(1..=12))
}

/// 从图片路径解析 `conv_hash`：`attach/<conv_hash>/<YYYY-MM>/Img/<file>`
pub fn conv_hash_for_path(images_root: &Path, file_path: &Path) -> Option<String> {
    let relative = file_path.strip_prefix(images_root).ok()?;
    let mut components = relative.components();
    let conv = components.next()?.as_os_str().to_str()?;
    let month = components.next()?.as_os_str().to_str()?;
    if !is_month_directory(month) {
        return None;
    }
    let img_dir = components.next()?.as_os_str().to_str()?;
    if !img_dir.eq_ignore_ascii_case("Img") {
        return None;
    }
    if conv.is_empty() {
        return None;
    }
    Some(conv.to_string())
}

/// 从图片路径解析月份 `YYYY-MM`
pub fn month_for_path(images_root: &Path, file_path: &Path) -> Option<String> {
    let relative = file_path.strip_prefix(images_root).ok()?;
    let mut components = relative.components();
    let _conv = components.next()?;
    let month = components.next()?.as_os_str().to_str()?;
    if is_month_directory(month) {
        Some(month.to_string())
    } else {
        None
    }
}

/// 从文件名启发式识别变体类型
///
/// 常见标记：`thumb`、`_hd`、尺寸标记；无法判断时返回 None。
pub fn variant_hint_from_filename(stem: &str) -> Option<MediaVariant> {
    let lower = stem.to_lowercase();
    if lower.contains("thumb") {
        return Some(MediaVariant::Thumbnail);
    }
    if lower.ends_with("_hd") || lower.contains("_hd_") || lower.ends_with("-hd") {
        return Some(MediaVariant::High);
    }
    if lower.contains("_display") || lower.ends_with("_display") {
        return Some(MediaVariant::Display);
    }
    None
}

/// 规范化 stem：去掉已知变体后缀
///
/// 去掉 `_thumb`、`_hd`、`_display` 及纯尺寸标记（如 `_640x480`、`_1080`）。
pub fn normalize_stem(stem: &str) -> String {
    let mut s = stem.to_string();
    // 反复剥离已知后缀
    loop {
        let lower = s.to_lowercase();
        let mut stripped = false;
        for suffix in ["_thumb", "-thumb", "_display", "-display", "_hd", "-hd"] {
            if lower.ends_with(suffix) {
                s.truncate(s.len() - suffix.len());
                stripped = true;
                break;
            }
        }
        // 尺寸标记：_640x480 / -640x480 / _1080
        if !stripped {
            if let Some((prefix, rest)) = s.rsplit_once(['_', '-']) {
                let is_size = rest
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == 'x' || c == 'X')
                    && rest.len() >= 2;
                if is_size && !prefix.is_empty() {
                    s = prefix.to_string();
                    stripped = true;
                }
            }
        }
        if !stripped {
            break;
        }
    }
    s
}

/// 计算分组键：blake3(source_type || account_id || conv_hash || month || normalized_stem)
pub fn compute_image_group_key(
    source_account_id: &str,
    conv_hash: &str,
    month: &str,
    normalized_stem: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(WECHAT_WINDOWS_4_SOURCE_TYPE.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_account_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(conv_hash.as_bytes());
    hasher.update(b"\0");
    hasher.update(month.as_bytes());
    hasher.update(b"\0");
    hasher.update(normalized_stem.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// 枚举账号图片根下 `attach/<conv_hash>/<YYYY-MM>/Img/` 的全部普通文件。
///
/// 首期完整枚举（不使用月份 mtime 裁剪）；严格限制在账号根内。
pub fn discover_image_candidates(
    images_dir: &Path,
    source_account_id: &str,
) -> Vec<ImageCandidate> {
    if !images_dir.is_dir() || is_link_or_reparse(images_dir) {
        return Vec::new();
    }
    // attach 位于 `<account>/msg/attach`；同时校验账号根是真实目录，避免
    // 通过账号目录或其父级重解析点把规范化路径带到选定账号之外。
    let account_root = images_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(images_dir);
    if is_link_or_reparse(account_root) {
        return Vec::new();
    }
    let canonical_account_root = match std::fs::canonicalize(account_root) {
        Ok(root) => root,
        Err(_) => return Vec::new(),
    };
    let canonical_root = match std::fs::canonicalize(images_dir) {
        Ok(root) => root,
        Err(_) => return Vec::new(),
    };
    if !canonical_root.starts_with(&canonical_account_root) {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let Ok(conv_entries) = std::fs::read_dir(images_dir) else {
        return candidates;
    };

    for conv_entry in conv_entries.flatten() {
        let conv_path = conv_entry.path();
        let Some(conv_path) = canonical_child(&canonical_root, &conv_path) else {
            continue;
        };
        if !conv_path.is_dir() {
            continue;
        }
        let conv_name = match conv_entry.file_name().to_str() {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };
        let Ok(month_entries) = std::fs::read_dir(&conv_path) else {
            continue;
        };
        for month_entry in month_entries.flatten() {
            let month_path = month_entry.path();
            let Some(month_path) = canonical_child(&canonical_root, &month_path) else {
                continue;
            };
            if !month_path.is_dir() {
                continue;
            }
            let month_name = match month_entry.file_name().to_str() {
                Some(n) if is_month_directory(n) => n.to_string(),
                _ => continue,
            };
            let img_dir = month_path.join("Img");
            let Some(img_dir) = canonical_child(&canonical_root, &img_dir) else {
                continue;
            };
            if !img_dir.is_dir() {
                continue;
            }
            let Ok(file_entries) = std::fs::read_dir(&img_dir) else {
                continue;
            };
            for file_entry in file_entries.flatten() {
                let path = file_entry.path();
                let Some(path) = canonical_child(&canonical_root, &path) else {
                    continue;
                };
                if !path.is_file() || !path.starts_with(&img_dir) {
                    continue;
                }
                let metadata = match std::fs::metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if metadata.len() == 0 {
                    continue;
                }
                let file_name = match path.file_name().and_then(|s| s.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let normalized = normalize_stem(&stem);
                let filename_variant = variant_hint_from_filename(&stem);
                let group_key = compute_image_group_key(
                    source_account_id,
                    &conv_name,
                    &month_name,
                    &normalized,
                );
                let modified_time: DateTime<Utc> =
                    metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH).into();

                candidates.push(ImageCandidate {
                    source_path: path,
                    file_name,
                    file_size: metadata.len(),
                    modified_time,
                    conv_hash: Some(conv_name.clone()),
                    month: month_name.clone(),
                    normalized_stem: normalized,
                    filename_variant,
                    image_group_key: group_key,
                });
            }
        }
    }

    candidates
}

/// 将图片候选转换为普通 DiscoveredFile（用于标准明文图片直接入库路径）
pub fn candidate_to_discovered(
    candidate: &ImageCandidate,
    source_account_id: &str,
) -> DiscoveredFile {
    DiscoveredFile {
        source_type: WECHAT_WINDOWS_4_SOURCE_TYPE.to_string(),
        source_account_id: Some(source_account_id.to_string()),
        absolute_path: candidate.source_path.to_string_lossy().to_string(),
        file_name: candidate.file_name.clone(),
        file_size: candidate.file_size,
        modified_time: candidate.modified_time,
        source_conversation_id: candidate.conv_hash.clone(),
    }
}

/// 根据解密后尺寸与文件名提示确定最终变体类型
///
/// 冲突时以尺寸为准；两者都不确定则 `Unknown`。
pub fn resolve_variant(
    filename_hint: Option<MediaVariant>,
    width: u32,
    height: u32,
    group_max_pixels: u64,
) -> MediaVariant {
    let pixels = width as u64 * height as u64;
    // 明显小图（小于组内最大图的 1/8 或边长 <= 200）判 thumbnail
    if pixels > 0 && (pixels.saturating_mul(8) < group_max_pixels || width.max(height) <= 200) {
        return MediaVariant::Thumbnail;
    }
    // 组内最大像素优先判 high
    if group_max_pixels > 0 && pixels >= group_max_pixels {
        return MediaVariant::High;
    }
    match filename_hint {
        Some(v) => v,
        None => MediaVariant::Display,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_conv_hash_and_month() {
        let root = Path::new(r"C:\x\msg\attach");
        let file = root
            .join("convabc")
            .join("2026-09")
            .join("Img")
            .join("a.dat");
        assert_eq!(conv_hash_for_path(root, &file).as_deref(), Some("convabc"));
        assert_eq!(month_for_path(root, &file).as_deref(), Some("2026-09"));
    }

    #[test]
    fn rejects_wrong_layout() {
        let root = Path::new(r"C:\x\msg\attach");
        let file = root
            .join("convabc")
            .join("notmonth")
            .join("Img")
            .join("a.dat");
        assert_eq!(conv_hash_for_path(root, &file), None);
    }

    #[test]
    fn normalizes_variant_suffixes() {
        assert_eq!(normalize_stem("photo_thumb"), "photo");
        assert_eq!(normalize_stem("photo_hd"), "photo");
        assert_eq!(normalize_stem("photo_display"), "photo");
        assert_eq!(normalize_stem("photo_640x480"), "photo");
        assert_eq!(normalize_stem("photo"), "photo");
        assert_eq!(normalize_stem("img_thumb_hd"), "img");
    }

    #[test]
    fn filename_variant_hints() {
        assert_eq!(
            variant_hint_from_filename("pic_thumb"),
            Some(MediaVariant::Thumbnail)
        );
        assert_eq!(
            variant_hint_from_filename("pic_hd"),
            Some(MediaVariant::High)
        );
        assert_eq!(variant_hint_from_filename("pic"), None);
    }

    #[test]
    fn discovers_candidates_in_layout() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("cv_img_disc_{unique}"));
        let img_dir = base.join("conv1").join("2026-09").join("Img");
        std::fs::create_dir_all(&img_dir).unwrap();
        std::fs::write(img_dir.join("photo.dat"), b"encrypted").unwrap();
        std::fs::write(img_dir.join("photo_thumb.dat"), b"encrypted-thumb").unwrap();
        std::fs::write(img_dir.join("empty.dat"), b"").unwrap();

        let candidates = discover_image_candidates(&base, "wxid_test");
        assert_eq!(candidates.len(), 2);
        let names: Vec<_> = candidates.iter().map(|c| c.file_name.as_str()).collect();
        assert!(names.contains(&"photo.dat"));
        assert!(names.contains(&"photo_thumb.dat"));

        let photo = candidates
            .iter()
            .find(|c| c.file_name == "photo.dat")
            .unwrap();
        assert_eq!(photo.conv_hash.as_deref(), Some("conv1"));
        assert_eq!(photo.month, "2026-09");
        assert_eq!(photo.normalized_stem, "photo");
        assert_eq!(photo.image_group_key, photo.image_group_key);

        // 同组 photo.dat 与 photo_thumb.dat 应共享分组键
        let thumb = candidates
            .iter()
            .find(|c| c.file_name == "photo_thumb.dat")
            .unwrap();
        assert_eq!(photo.image_group_key, thumb.image_group_key);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn resolves_thumbnail_by_size() {
        let v = resolve_variant(None, 100, 100, 1_000_000);
        assert_eq!(v, MediaVariant::Thumbnail);
        let v = resolve_variant(None, 1920, 1080, 1920 * 1080);
        assert_eq!(v, MediaVariant::High);
    }
}
