// ChatVault Vault 配置远端初始化
// 负责将 vault.json 写入 WebDAV，保证多设备读取同一格式版本与哈希算法

use crate::client::WebDavClient;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::VaultConfig;
use chatvault_metadata::get_config_path;

/// 确保远端存在有效的 Vault 配置文件
///
/// 职责:
///   - 若远端已存在 vault.json 则复用
///   - 否则以本地 VaultConfig 序列化后 PUT 创建
/// 输入:
///   - `client`: WebDAV 客户端
///   - `config`: 本地 Vault 配置
/// 输出: `Result<()>`
pub async fn ensure_vault_config(client: &WebDavClient, config: &VaultConfig) -> Result<()> {
    validate_config(config, &config.vault_id)?;
    let path = get_config_path(&config.vault_id);

    if client.exists(&path).await? {
        load_vault_config(client, &config.vault_id).await?;
        tracing::info!("远端 Vault 配置已存在，跳过写入: {}", path);
        return Ok(());
    }

    let bytes = serde_json::to_vec_pretty(config)?;
    // 并发初始化时 created_at 允许不同，但协议身份必须一致。
    if let Err(e) = client.upload_immutable(&bytes, &path).await {
        if client.exists(&path).await? {
            load_vault_config(client, &config.vault_id).await?;
        } else {
            return Err(e);
        }
    }
    tracing::info!("已写入远端 Vault 配置: {}", path);
    Ok(())
}

/// 从远端读取 Vault 配置
///
/// 输入:
///   - `client`: WebDAV 客户端
///   - `vault_id`: 资料库标识
/// 输出: `Result<VaultConfig>`
pub async fn load_vault_config(client: &WebDavClient, vault_id: &str) -> Result<VaultConfig> {
    chatvault_metadata::validate_vault_id(vault_id)?;
    let path = get_config_path(vault_id);
    let resp = client.get_stream(&path).await?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ChatVaultError::WebDav(format!("读取 Vault 配置失败: {}", e)))?;
    let config: VaultConfig = serde_json::from_slice(&bytes)?;
    validate_config(&config, vault_id)?;
    Ok(config)
}

/// 已有 Vault 只有身份、格式版本和算法都一致时才允许读写。
fn validate_config(config: &VaultConfig, vault_id: &str) -> Result<()> {
    chatvault_metadata::validate_vault_id(vault_id)?;
    if config.vault_id != vault_id
        || config.format_version != 1
        || config.hash_algorithm != "blake3"
    {
        return Err(ChatVaultError::WebDav(
            "Vault 身份或格式不兼容，已停止操作".into(),
        ));
    }
    Ok(())
}
