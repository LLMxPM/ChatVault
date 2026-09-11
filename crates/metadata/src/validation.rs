// ChatVault 协议字段校验：限制路径标识与完整内容哈希。
use chatvault_core::error::{ChatVaultError, Result};

/// 校验单段标识，禁止路径穿越和 URL 控制字符。
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ChatVaultError::Internal(
            "标识只能包含字母、数字、下划线和连字符，长度为 1～128".into(),
        ));
    }
    Ok(())
}

/// 校验 BLAKE3 完整小写十六进制哈希，避免截断或非法路径。
pub fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ChatVaultError::Internal(
            "BLAKE3 必须为 64 位小写十六进制哈希".into(),
        ));
    }
    Ok(())
}
