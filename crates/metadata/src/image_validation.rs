//! # 完整图片格式校验
//!
//! 使用标准解码器对明文图片做完整校验；魔数命中只用于筛选，
//! 不能作为备份成功标准。禁用截断容忍，限制尺寸与内存。

use chatvault_core::error::{ChatVaultError, Result};
use image::GenericImageView;
use std::io::{Cursor, Read};
use std::path::Path;

/// 图片校验结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedImage {
    /// 宽
    pub width: u32,
    /// 高
    pub height: u32,
    /// 帧数（静态图为 1）
    pub frame_count: u32,
    /// 检测到的 MIME
    pub mime: String,
    /// 建议扩展名（不含点，小写）
    pub extension: String,
    /// 字节大小
    pub size: u64,
}

/// 最大允许的单边像素（防止解码炸弹）
const MAX_DIMENSION: u32 = 20_000;
/// 最大允许帧数
const MAX_FRAMES: u32 = 10_000;
/// 从磁盘读取图片时的单文件上限。
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// 根据魔数快速判断是否可能是标准图片（用于筛选，非成功标准）
pub fn looks_like_standard_image(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    // JPEG
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return true;
    }
    // PNG
    if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return true;
    }
    // GIF
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return true;
    }
    // WebP (RIFF....WEBP)
    if data.starts_with(b"RIFF") && data.len() >= 12 && &data[8..12] == b"WEBP" {
        return true;
    }
    // BMP
    if data.starts_with(b"BM") {
        return true;
    }
    false
}

/// 是否为 WXGF（微信私有容器，未验收前不计入成功）
pub fn looks_like_wxgf(data: &[u8]) -> bool {
    data.starts_with(b"WXGF") || data.starts_with(&[0x00, 0x00, 0x00, 0x01])
}

/// 完整校验内存中的图片字节。
///
/// 使用标准解码器严格解码；失败返回错误，不以魔数命中掩盖。
pub fn validate_image_bytes(data: &[u8]) -> Result<ValidatedImage> {
    if data.is_empty() {
        return Err(ChatVaultError::WeChatParse("invalid_image: 空内容".into()));
    }
    if looks_like_wxgf(data) {
        return Err(ChatVaultError::WeChatParse(
            "unsupported_payload: WXGF".into(),
        ));
    }

    let format = image::guess_format(data)
        .map_err(|_| ChatVaultError::WeChatParse("unsupported_payload: 无法识别图片格式".into()))?;

    // 完整解码：image::load_from_memory 会对所有支持格式做严格解码
    let img = image::load_from_memory(data).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("format") || msg.contains("unsupported") {
            ChatVaultError::WeChatParse(format!("unsupported_payload: {msg}"))
        } else {
            ChatVaultError::WeChatParse(format!("invalid_image: {msg}"))
        }
    })?;

    let (width, height) = img.dimensions();
    check_dimensions(width, height)?;

    // 动画必须完整读取每一帧；image::load_from_memory 可能只解出首帧，不能单独作为完整性证明。
    let frame_count = match format {
        image::ImageFormat::Gif => {
            use image::AnimationDecoder;
            let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(data))
                .map_err(|e| ChatVaultError::WeChatParse(format!("invalid_image: {e}")))?
                .into_frames();
            let mut n = 0u32;
            for frame in decoder {
                frame.map_err(|e| ChatVaultError::WeChatParse(format!("invalid_image: {e}")))?;
                n = n.saturating_add(1);
                if n > MAX_FRAMES {
                    return Err(ChatVaultError::WeChatParse(
                        "invalid_image: 帧数超限或为空".into(),
                    ));
                }
            }
            if n == 0 {
                return Err(ChatVaultError::WeChatParse(
                    "invalid_image: 帧数超限或为空".into(),
                ));
            }
            n
        }
        image::ImageFormat::WebP => {
            use image::AnimationDecoder;
            let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(data))
                .map_err(|e| ChatVaultError::WeChatParse(format!("unsupported_payload: {e}")))?;
            if decoder.has_animation() {
                let mut n = 0u32;
                for frame in decoder.into_frames() {
                    frame.map_err(|e| {
                        ChatVaultError::WeChatParse(format!("unsupported_payload: {e}"))
                    })?;
                    n = n.saturating_add(1);
                    if n > MAX_FRAMES {
                        return Err(ChatVaultError::WeChatParse(
                            "invalid_image: 帧数超限或为空".into(),
                        ));
                    }
                }
                if n == 0 {
                    return Err(ChatVaultError::WeChatParse(
                        "invalid_image: 帧数超限或为空".into(),
                    ));
                }
                n
            } else {
                1
            }
        }
        _ => 1,
    };

    let (mime, extension) = match format {
        image::ImageFormat::Png => ("image/png".to_string(), "png".to_string()),
        image::ImageFormat::Jpeg => ("image/jpeg".to_string(), "jpg".to_string()),
        image::ImageFormat::Gif => ("image/gif".to_string(), "gif".to_string()),
        image::ImageFormat::WebP => ("image/webp".to_string(), "webp".to_string()),
        image::ImageFormat::Bmp => ("image/bmp".to_string(), "bmp".to_string()),
        other => {
            return Err(ChatVaultError::WeChatParse(format!(
                "unsupported_payload: {:?}",
                other
            )))
        }
    };

    Ok(ValidatedImage {
        width,
        height,
        frame_count,
        mime,
        extension,
        size: data.len() as u64,
    })
}

/// 校验磁盘上的图片文件
pub fn validate_image_file<P: AsRef<Path>>(path: P) -> Result<ValidatedImage> {
    let metadata = std::fs::metadata(path.as_ref())?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(ChatVaultError::WeChatParse(
            "invalid_image: 文件超过读取上限".into(),
        ));
    }
    let mut data = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(path.as_ref())?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > MAX_FILE_BYTES {
        return Err(ChatVaultError::WeChatParse(
            "invalid_image: 文件超过读取上限".into(),
        ));
    }
    validate_image_bytes(&data)
}

fn check_dimensions(w: u32, h: u32) -> Result<()> {
    if w == 0 || h == 0 || w > MAX_DIMENSION || h > MAX_DIMENSION {
        return Err(ChatVaultError::WeChatParse(
            "invalid_image: 尺寸超出允许范围".into(),
        ));
    }
    Ok(())
}

/// 推断普通文件的 MIME（用于非图片路径的简化场景）
pub fn mime_from_extension(ext: &str) -> String {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg".into(),
        "png" => "image/png".into(),
        "gif" => "image/gif".into(),
        "webp" => "image/webp".into(),
        "bmp" => "image/bmp".into(),
        "mp4" => "video/mp4".into(),
        "pdf" => "application/pdf".into(),
        "txt" => "text/plain".into(),
        "zip" => "application/zip".into(),
        _ => format!("application/{}", ext.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成一个最小合法 PNG（1x1 红色像素）
    fn minimal_png() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn validates_minimal_png() {
        let png = minimal_png();
        let result = validate_image_bytes(&png).unwrap();
        assert_eq!(result.width, 1);
        assert_eq!(result.height, 1);
        assert_eq!(result.extension, "png");
        assert_eq!(result.mime, "image/png");
        assert_eq!(result.frame_count, 1);
    }

    #[test]
    fn rejects_empty() {
        assert!(validate_image_bytes(&[]).is_err());
    }

    #[test]
    fn rejects_wxgf() {
        let data = b"WXGF\x00\x00\x00\x00\x00\x00\x00\x00";
        let err = validate_image_bytes(data).unwrap_err();
        assert!(err.to_string().contains("unsupported_payload"));
    }

    #[test]
    fn rejects_truncated_png() {
        let png = minimal_png();
        let truncated = &png[..png.len() / 2];
        assert!(validate_image_bytes(truncated).is_err());
    }

    #[test]
    fn rejects_truncated_gif_and_webp() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            2,
            2,
            image::Rgb([1, 2, 3]),
        ));
        let mut gif_buf = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut gif_buf, image::ImageFormat::Gif)
            .unwrap();
        let gif = gif_buf.into_inner();
        assert!(validate_image_bytes(&gif[..gif.len().saturating_sub(2)]).is_err());

        let mut webp_buf = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut webp_buf, image::ImageFormat::WebP)
            .unwrap();
        let webp = webp_buf.into_inner();
        assert!(validate_image_bytes(&webp[..webp.len().saturating_sub(2)]).is_err());
    }

    #[test]
    fn detects_standard_image_magic() {
        let png = minimal_png();
        assert!(looks_like_standard_image(&png));
        assert!(!looks_like_standard_image(b"not an image at all!!"));
    }
}
