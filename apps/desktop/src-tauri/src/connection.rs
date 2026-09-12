// ChatVault 连接配置命令：同步页与设置页共用同一持久化配置和凭据键。
use crate::{commands::WebdavConfigDto, state::AppState};
use chatvault_webdav::{credential_key, WebDavClient, WebDavConfig};
use tauri::State;

/// 保存连接参数；已有索引不允许切换 Vault，密码仅进入系统凭据库。
#[tauri::command]
pub async fn save_webdav_config(
    config: WebdavConfigDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = WebDavClient::new(WebDavConfig {
        base_url: config.url.clone(),
        username: None,
        password: None,
    })
    .map_err(|e| e.to_string())?;
    let url = client.storage_identity().to_string();
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    db.check_remote_binding(&url, &config.vault_id)
        .map_err(|e| e.to_string())?;
    if let Some(password) = config.password.filter(|p| !p.is_empty()) {
        let key = credential_key(&url, &config.username);
        keyring::Entry::new("chatvault-webdav", &key)
            .and_then(|e| e.set_password(&password))
            .map_err(|e| e.to_string())?;
    }
    db.atomic(|db| {
        db.connection()
            .execute(
                "UPDATE app_settings SET value=value WHERE key='remote_binding'",
                [],
            )
            .map_err(|e| chatvault_core::error::ChatVaultError::Database(e.to_string()))?;
        db.check_remote_binding(&url, &config.vault_id)?;
        db.set_setting("webdav_url", &url)?;
        db.set_setting("webdav_username", config.username.trim())?;
        db.set_setting("vault_id", &config.vault_id)
    })
    .map_err(|e| e.to_string())
}
