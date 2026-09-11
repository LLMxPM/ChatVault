// ChatVault 桌面命令：sync 职责实现与前端错误映射。
use super::webdav::resolve_webdav_password;
use super::*;

/// 同步操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResultDto {
    pub published_seq: Option<u64>,
    pub applied_events: usize,
    pub total_records: usize,
    pub total_objects: usize,
    pub message: String,
}

/// 发布本机元数据日志到 WebDAV
#[tauri::command]
pub async fn sync_publish(
    config: WebdavConfigDto,
    state: State<'_, AppState>,
) -> std::result::Result<SyncResultDto, String> {
    let password =
        resolve_webdav_password(&config.url, &config.username, config.password.as_deref());
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let mut db = state.get_db().map_err(|e| e.to_string())?;

    let cfg = WebDavConfig {
        base_url: config.url.clone(),
        username: Some(config.username.clone()),
        password,
    };
    let client = WebDavClient::new(cfg).map_err(|e| e.to_string())?;
    let seq =
        chatvault_sync::publish_pending_events(&client, &mut db, &config.vault_id, &device_id)
            .await
            .map_err(|e| e.to_string())?;
    let stats = db.get_stats().map_err(|e| e.to_string())?;

    Ok(SyncResultDto {
        published_seq: Some(seq),
        applied_events: 0,
        total_records: stats.total_records,
        total_objects: stats.total_objects,
        message: format!("本机日志已发布，游标 seq={}", seq),
    })
}

/// 拉取远端日志并合并
#[tauri::command]
pub async fn sync_pull(
    config: WebdavConfigDto,
    state: State<'_, AppState>,
) -> std::result::Result<SyncResultDto, String> {
    let password =
        resolve_webdav_password(&config.url, &config.username, config.password.as_deref());
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let mut db = state.get_db().map_err(|e| e.to_string())?;

    let cfg = WebDavConfig {
        base_url: config.url.clone(),
        username: Some(config.username.clone()),
        password,
    };
    let client = WebDavClient::new(cfg).map_err(|e| e.to_string())?;
    let applied = chatvault_sync::pull_and_apply(&client, &mut db, &config.vault_id, &device_id)
        .await
        .map_err(|e| e.to_string())?;
    let stats = db.get_stats().map_err(|e| e.to_string())?;

    Ok(SyncResultDto {
        published_seq: None,
        applied_events: applied,
        total_records: stats.total_records,
        total_objects: stats.total_objects,
        message: format!("已应用 {} 条远端事件", applied),
    })
}

/// 从远端恢复空索引
#[tauri::command]
pub async fn sync_restore(
    config: WebdavConfigDto,
    state: State<'_, AppState>,
) -> std::result::Result<SyncResultDto, String> {
    let password =
        resolve_webdav_password(&config.url, &config.username, config.password.as_deref());
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let mut db = state.get_db().map_err(|e| e.to_string())?;

    let cfg = WebDavConfig {
        base_url: config.url.clone(),
        username: Some(config.username.clone()),
        password,
    };
    let client = WebDavClient::new(cfg).map_err(|e| e.to_string())?;
    let report =
        chatvault_sync::restore_from_remote(&client, &mut db, &config.vault_id, &device_id)
            .await
            .map_err(|e| e.to_string())?;

    Ok(SyncResultDto {
        published_seq: None,
        applied_events: report.applied_events,
        total_records: report.total_records,
        total_objects: report.total_objects,
        message: format!(
            "恢复完成：应用事件 {}, 记录 {}, 对象 {}",
            report.applied_events, report.total_records, report.total_objects
        ),
    })
}
