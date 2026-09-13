// ChatVault 协议字段校验：限制路径标识与完整内容哈希。
use chatvault_core::error::{ChatVaultError, Result};

/// Vault ID 固定前缀；后缀由用户配置，组成完整 `chatvault-xxxx`
pub const VAULT_ID_PREFIX: &str = "chatvault-";

/// 用可配置后缀组装完整 Vault ID
pub fn format_vault_id(suffix: &str) -> String {
    format!("{}{}", VAULT_ID_PREFIX, suffix)
}

/// 提取 Vault ID 的可配置后缀；无前缀时原样返回
pub fn vault_id_suffix(vault_id: &str) -> &str {
    vault_id.strip_prefix(VAULT_ID_PREFIX).unwrap_or(vault_id)
}

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

/// 校验完整 Vault ID：必须为 `chatvault-` + 合法后缀。
pub fn validate_vault_id(id: &str) -> Result<()> {
    let Some(suffix) = id.strip_prefix(VAULT_ID_PREFIX) else {
        return Err(ChatVaultError::Internal(
            "Vault ID 必须以 chatvault- 开头".into(),
        ));
    };
    validate_id(suffix)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_id_requires_prefix() {
        assert!(validate_vault_id("chatvault-home").is_ok());
        assert!(validate_vault_id("chatvault-default").is_ok());
        assert!(validate_vault_id("chatvault-").is_err());
        assert!(validate_vault_id("home").is_err());
        assert!(validate_vault_id("ChatVault-home").is_err());
        assert!(validate_vault_id("chatvault-a/b").is_err());
    }

    #[test]
    fn vault_suffix_helpers() {
        assert_eq!(format_vault_id("home"), "chatvault-home");
        assert_eq!(vault_id_suffix("chatvault-home"), "home");
        assert_eq!(vault_id_suffix("plain"), "plain");
    }
}
