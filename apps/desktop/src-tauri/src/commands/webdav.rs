// ChatVault 桌面命令：webdav 职责实现与前端错误映射。
use super::*;
use chatvault_webdav::credential_key as webdav_credential_key;

/// 辅助函数：优先读取入参密码，若为空则从操作系统安全凭据管理器检索
pub(super) fn resolve_webdav_password(
    url: &str,
    username: &str,
    input_pass: Option<&str>,
) -> Option<String> {
    if let Some(p) = input_pass {
        let trimmed = p;
        if !trimmed.is_empty() {
            // 如果用户显式输入了密码，自动同步存入系统凭据管理器
            let key = webdav_credential_key(url, username);
            if let Ok(entry) = keyring::Entry::new("chatvault-webdav", &key) {
                let _ = entry.set_password(trimmed);
            }
            return Some(trimmed.to_string());
        }
    }

    // 从系统凭据管理器查询
    let key = webdav_credential_key(url, username);
    keyring::Entry::new("chatvault-webdav", &key)
        .and_then(|entry| entry.get_password())
        .ok()
}

/// 保存 WebDAV 密码到系统凭据管理器
#[tauri::command]
pub async fn save_webdav_credential(
    url: String,
    username: String,
    password: String,
) -> std::result::Result<(), String> {
    let key = webdav_credential_key(&url, &username);
    let entry = keyring::Entry::new("chatvault-webdav", &key).map_err(|e| e.to_string())?;
    entry.set_password(&password).map_err(|e| e.to_string())?;
    Ok(())
}

/// 从系统凭据管理器读取 WebDAV 密码
#[tauri::command]
pub async fn load_webdav_credential(
    url: String,
    username: String,
) -> std::result::Result<String, String> {
    let key = webdav_credential_key(&url, &username);
    let entry = keyring::Entry::new("chatvault-webdav", &key).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

/// 从系统凭据管理器删除 WebDAV 密码；不存在时视为成功（幂等）
#[tauri::command]
pub async fn clear_webdav_credential(
    url: String,
    username: String,
) -> std::result::Result<(), String> {
    let key = webdav_credential_key(&url, &username);
    let entry = keyring::Entry::new("chatvault-webdav", &key).map_err(|e| e.to_string())?;
    match entry.delete_password() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// 测试 WebDAV 连接与 RFC4918 协议能力
#[tauri::command]
pub async fn test_webdav(
    config: WebdavConfigDto,
) -> std::result::Result<WebdavCapabilityDto, String> {
    let password =
        resolve_webdav_password(&config.url, &config.username, config.password.as_deref());

    let cfg = WebDavConfig {
        base_url: config.url.clone(),
        username: Some(config.username.clone()),
        password,
    };

    let start = Instant::now();
    let client = WebDavClient::new(cfg).map_err(|e| e.to_string())?;
    let detector = CapabilityDetector::new(&client);

    match detector.detect().await {
        Ok(report) => Ok(WebdavCapabilityDto {
            reachable: report.reachable,
            authenticated: report.authenticated,
            support_mkcol: report.support_mkcol,
            support_move: report.support_move,
            message: report.message,
            duration_ms: start.elapsed().as_millis(),
        }),
        Err(e) => Ok(WebdavCapabilityDto {
            reachable: false,
            authenticated: false,
            support_mkcol: false,
            support_move: false,
            message: format!("WebDAV 连通性测试未通过: {}", e),
            duration_ms: start.elapsed().as_millis(),
        }),
    }
}

/// 执行对象归档推送到 WebDAV 并验证远端回读哈希
#[tauri::command]
pub async fn archive_to_webdav(
    config: WebdavConfigDto,
    state: State<'_, AppState>,
) -> std::result::Result<ArchiveResultDto, String> {
    let start = Instant::now();
    let password =
        resolve_webdav_password(&config.url, &config.username, config.password.as_deref());

    let cfg = WebDavConfig {
        base_url: config.url.clone(),
        username: Some(config.username.clone()),
        password,
    };

    let client = WebDavClient::new(cfg).map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let report =
        chatvault_sync::archive::archive_pending(&client, &mut db, &config.vault_id, &device_id, 0)
            .await
            .map_err(|e| e.to_string())?;
    Ok(ArchiveResultDto {
        uploaded_count: report.uploaded,
        verified_count: report.verified,
        failed_count: report.failed,
        duration_ms: start.elapsed().as_millis(),
    })
}
