//! # 微信 V2 加密图片三段结构解析与解密
//!
//! 三段模型：头 15 字节 + AES 密文 + 原样中间段 + XOR 尾段。
//! AES-128-ECB + 严格 PKCS#7；解密失败报告具体错误，不以魔数命中掩盖完整性失败。

use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes128;
use chatvault_core::error::ChatVaultError;

/// V2 签名前 6 字节：`07 08 56 32 08 07`
pub const V2_SIGNATURE: [u8; 6] = [0x07, 0x08, 0x56, 0x32, 0x08, 0x07];

/// V2 头部最小长度
pub const V2_HEADER_LEN: usize = 15;

/// V2 解密失败原因（不携带密钥或明文片段）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2DecryptError {
    /// 文件过短，不足以容纳完整头部
    TooShort,
    /// 签名不匹配（非 V2 或未知变体）
    SignatureMismatch,
    /// 长度字段导致溢出或边界非法
    InvalidLengths,
    /// 中间原样段长度为负（段重叠）
    OverlappingSegments,
    /// AES 解密后 PKCS#7 填充非法或去填充后长度不等于 A
    InvalidPadding,
    /// AES 块对齐错误
    InvalidAesBlock,
    /// 解密后载荷不是受支持的标准图片
    UnsupportedPayload,
}

impl std::fmt::Display for V2DecryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            V2DecryptError::TooShort => write!(f, "文件过短"),
            V2DecryptError::SignatureMismatch => write!(f, "非 V2 签名"),
            V2DecryptError::InvalidLengths => write!(f, "长度字段非法"),
            V2DecryptError::OverlappingSegments => write!(f, "段边界重叠"),
            V2DecryptError::InvalidPadding => write!(f, "PKCS#7 填充非法"),
            V2DecryptError::InvalidAesBlock => write!(f, "AES 块对齐错误"),
            V2DecryptError::UnsupportedPayload => write!(f, "不支持的载荷格式"),
        }
    }
}

/// V2 头部解析结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V2Header {
    /// AES 明文段目标长度 A
    pub aes_plain_len: u32,
    /// XOR 尾段长度 X
    pub xor_tail_len: u32,
    /// 标志字节（已验证样本均为 1）
    pub flag: u8,
    /// AES 密文段长度 C = 16 × (floor(A/16) + 1)
    pub aes_cipher_len: usize,
    /// 中间原样段长度 R = L - 15 - C - X
    pub raw_mid_len: usize,
}

/// 解析 V2 三段结构头；输入为完整密文字节。
pub fn parse_v2_header(data: &[u8]) -> std::result::Result<V2Header, V2DecryptError> {
    if data.len() < V2_HEADER_LEN {
        return Err(V2DecryptError::TooShort);
    }
    if data[..6] != V2_SIGNATURE {
        return Err(V2DecryptError::SignatureMismatch);
    }

    let aes_plain_len = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
    let xor_tail_len = u32::from_le_bytes([data[10], data[11], data[12], data[13]]);
    let flag = data[14];

    // 已验证样本标志字节均为 1；未知标志明确拒绝
    if flag != 1 {
        return Err(V2DecryptError::InvalidLengths);
    }
    // A 必须为正
    if aes_plain_len == 0 {
        return Err(V2DecryptError::InvalidLengths);
    }

    let l = data.len();
    let c = (aes_plain_len as usize / 16 + 1) * 16;
    // R = L - 15 - C - X，必须 >= 0
    let Some(raw_mid_len) = l
        .checked_sub(V2_HEADER_LEN)
        .and_then(|rest| rest.checked_sub(c))
        .and_then(|rest| rest.checked_sub(xor_tail_len as usize))
    else {
        return Err(V2DecryptError::OverlappingSegments);
    };

    Ok(V2Header {
        aes_plain_len,
        xor_tail_len,
        flag,
        aes_cipher_len: c,
        raw_mid_len,
    })
}

/// 严格 PKCS#7 去填充；填充字节值必须等于填充长度，且全部一致。
fn strip_pkcs7(block: &[u8]) -> std::result::Result<&[u8], V2DecryptError> {
    if block.is_empty() {
        return Err(V2DecryptError::InvalidPadding);
    }
    let pad = *block.last().unwrap() as usize;
    if pad == 0 || pad > 16 || pad > block.len() {
        return Err(V2DecryptError::InvalidPadding);
    }
    let start = block.len() - pad;
    if !block[start..].iter().all(|&b| b as usize == pad) {
        return Err(V2DecryptError::InvalidPadding);
    }
    Ok(&block[..start])
}

/// 用 AES-128-ECB 解密密文段并严格去填充。
#[allow(clippy::chunks_exact_to_as_chunks, clippy::manual_is_multiple_of)]
fn decrypt_aes_ecb(
    key: &[u8; 16],
    cipher_bytes: &[u8],
    expected_plain_len: u32,
) -> std::result::Result<Vec<u8>, V2DecryptError> {
    if cipher_bytes.len() % 16 != 0 || cipher_bytes.is_empty() {
        return Err(V2DecryptError::InvalidAesBlock);
    }
    let cipher = Aes128::new(key.into());
    let mut buf = cipher_bytes.to_vec();
    for chunk in buf.chunks_exact_mut(16) {
        cipher.decrypt_block(chunk.into());
    }
    let unpadded = strip_pkcs7(&buf)?;
    if unpadded.len() != expected_plain_len as usize {
        return Err(V2DecryptError::InvalidPadding);
    }
    Ok(unpadded.to_vec())
}

/// XOR 还原尾段
fn xor_decode(data: &[u8], xor_byte: u8) -> Vec<u8> {
    data.iter().map(|b| b ^ xor_byte).collect()
}

/// 完整 V2 解密：解析头 → AES 解密去填充 → 拼接原样段 → XOR 还原尾段。
///
/// 输入密文全量字节与任务内存内的密钥材料；输出明文图片字节。
pub fn decrypt_v2(
    data: &[u8],
    aes_key: &[u8; 16],
    xor_byte: u8,
) -> std::result::Result<Vec<u8>, V2DecryptError> {
    let header = parse_v2_header(data)?;

    let aes_start = V2_HEADER_LEN;
    let aes_end = aes_start + header.aes_cipher_len;
    let mid_end = aes_end + header.raw_mid_len;
    let xor_end = mid_end + header.xor_tail_len as usize;

    // 边界必须精确落在文件末尾
    if xor_end != data.len() {
        return Err(V2DecryptError::InvalidLengths);
    }

    let aes_plain = decrypt_aes_ecb(aes_key, &data[aes_start..aes_end], header.aes_plain_len)?;
    let raw_mid = data[aes_end..mid_end].to_vec();
    let xor_tail = xor_decode(&data[mid_end..xor_end], xor_byte);

    let mut out = Vec::with_capacity(aes_plain.len() + raw_mid.len() + xor_tail.len());
    out.extend_from_slice(&aes_plain);
    out.extend_from_slice(&raw_mid);
    out.extend_from_slice(&xor_tail);
    Ok(out)
}

/// 将 V2 错误映射为领域错误码（不含敏感输入）
pub fn map_v2_error(err: V2DecryptError) -> ChatVaultError {
    match err {
        V2DecryptError::TooShort
        | V2DecryptError::SignatureMismatch
        | V2DecryptError::InvalidLengths
        | V2DecryptError::OverlappingSegments
        | V2DecryptError::InvalidPadding
        | V2DecryptError::InvalidAesBlock => {
            ChatVaultError::WeChatParse(format!("unsupported_structure: {err}"))
        }
        V2DecryptError::UnsupportedPayload => {
            ChatVaultError::WeChatParse("unsupported_payload".into())
        }
    }
}

/// 判断字节是否具备 V2 签名
pub fn looks_like_v2(data: &[u8]) -> bool {
    data.len() >= 6 && data[..6] == V2_SIGNATURE
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncrypt, KeyInit};
    use aes::Aes128;

    /// 构造合成 V2 密文：AES 明文段 + 原样段 + XOR 尾段
    #[allow(clippy::manual_repeat_n, clippy::chunks_exact_to_as_chunks)]
    fn build_v2(
        aes_plain: &[u8],
        raw_mid: &[u8],
        xor_plain: &[u8],
        key: &[u8; 16],
        xor_byte: u8,
    ) -> Vec<u8> {
        assert!(!aes_plain.is_empty());
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
        out.extend_from_slice(&V2_SIGNATURE);
        out.extend_from_slice(&(aes_plain.len() as u32).to_le_bytes());
        out.extend_from_slice(&(xor_enc.len() as u32).to_le_bytes());
        out.push(1u8);
        out.extend_from_slice(&encrypted);
        out.extend_from_slice(raw_mid);
        out.extend_from_slice(&xor_enc);
        out
    }

    fn test_key() -> [u8; 16] {
        *b"0123456789abcdef"
    }

    #[test]
    fn roundtrip_three_segments() {
        let key = test_key();
        let aes_plain = b"HELLO_AES_PLAIN!!"; // 17 bytes
        let raw_mid = b"MID";
        let xor_plain = b"xor-tail-bytes";
        let xor_byte = 0x5Au8;
        let cipher = build_v2(aes_plain, raw_mid, xor_plain, &key, xor_byte);

        let header = parse_v2_header(&cipher).unwrap();
        assert_eq!(header.aes_plain_len, 17);
        assert_eq!(header.xor_tail_len, xor_plain.len() as u32);
        assert_eq!(header.raw_mid_len, 3);
        assert_eq!(header.aes_cipher_len, 32);

        let plain = decrypt_v2(&cipher, &key, xor_byte).unwrap();
        assert_eq!(&plain[..17], aes_plain);
        assert_eq!(&plain[17..20], raw_mid);
        assert_eq!(&plain[20..], xor_plain);
    }

    #[test]
    fn empty_raw_mid_allowed() {
        let key = test_key();
        let aes_plain = b"0123456789abcdef"; // 16
        let cipher = build_v2(aes_plain, b"", b"tail", &key, 1);
        let plain = decrypt_v2(&cipher, &key, 1).unwrap();
        assert_eq!(&plain[..16], aes_plain);
        assert_eq!(&plain[16..], b"tail");
    }

    #[test]
    fn rejects_wrong_signature() {
        let key = test_key();
        let mut cipher = build_v2(b"12345678", b"", b"t", &key, 0);
        cipher[0] = 0xFF;
        assert_eq!(
            parse_v2_header(&cipher),
            Err(V2DecryptError::SignatureMismatch)
        );
    }

    #[test]
    fn rejects_zero_aes_len() {
        let key = test_key();
        let mut cipher = build_v2(b"12345678", b"", b"t", &key, 0);
        // A = 0
        cipher[6] = 0;
        cipher[7] = 0;
        cipher[8] = 0;
        cipher[9] = 0;
        assert_eq!(
            parse_v2_header(&cipher),
            Err(V2DecryptError::InvalidLengths)
        );
    }

    #[test]
    fn rejects_too_short() {
        assert_eq!(
            parse_v2_header(&[0x07, 0x08]),
            Err(V2DecryptError::TooShort)
        );
    }

    #[test]
    fn rejects_overlapping_segments() {
        // X 大于剩余长度
        let key = test_key();
        let mut cipher = build_v2(b"12345678", b"", b"t", &key, 0);
        let l = cipher.len() as u32;
        // 设置 X 为超大值
        cipher[10] = 0xFF;
        cipher[11] = 0xFF;
        cipher[12] = 0x00;
        cipher[13] = 0x00;
        assert_eq!(
            parse_v2_header(&cipher),
            Err(V2DecryptError::OverlappingSegments)
        );
        let _ = l;
    }

    #[test]
    fn rejects_bad_padding() {
        let key = test_key();
        let aes_plain = b"0123456789abcdef"; // 16，恰好整块，pad=16
        let mut cipher = build_v2(aes_plain, b"", b"", &key, 0);
        // 破坏最后一个密文块使填充非法
        let last = cipher.len() - 1;
        cipher[last] ^= 0xFF;
        assert!(decrypt_v2(&cipher, &key, 0).is_err());
    }

    #[test]
    fn rejects_unknown_flag() {
        let key = test_key();
        let mut cipher = build_v2(b"12345678", b"", b"t", &key, 0);
        cipher[14] = 2;
        assert_eq!(
            parse_v2_header(&cipher),
            Err(V2DecryptError::InvalidLengths)
        );
    }

    #[test]
    fn debug_material_is_redacted() {
        use crate::media::parameters::derive_key_material;
        let m = derive_key_material(12345, "wxid_abc_a1b2");
        let s = format!("{:?}", m);
        // 字段值必须被脱敏为 redacted
        assert!(s.contains("aes_key: \"[redacted]\""));
        assert!(s.contains("xor_byte: \"[redacted]\""));
        assert!(s.contains("code: \"[redacted]\""));
    }
}
