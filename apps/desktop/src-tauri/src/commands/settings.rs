// ChatVault 桌面命令：settings 职责实现与前端错误映射。
use super::*;
use chatvault_index::cache_policy::{CACHE_MAX_MIB, CACHE_RETENTION_DAYS, COPY_THRESHOLD_MIB};

/// 应用设置 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDto {
    pub vault_id: String,
    pub device_id: String,
    pub webdav_url: String,
    pub webdav_username: String,
    pub scan_interval_minutes: u32,
    pub schedule_enabled: bool,
    pub copy_threshold_mib: u32,
    pub cache_retention_days: u32,
    pub cache_max_mib: u32,
    pub collect_dirs: Vec<String>,
}

/// 读取全部应用设置
#[tauri::command]
pub async fn get_app_settings(
    state: State<'_, AppState>,
) -> std::result::Result<AppSettingsDto, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let get = |k: &str| db.get_setting(k).map_err(|e| e.to_string()).ok().flatten();
    let vault_id = state.vault_id().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;

    let collect_dirs_raw = get(setting_keys::COLLECT_DIRS).unwrap_or_else(|| "[]".to_string());
    let collect_dirs: Vec<String> = serde_json::from_str(&collect_dirs_raw).unwrap_or_default();

    let policy = db.cache_policy().map_err(|e| e.to_string())?;
    Ok(AppSettingsDto {
        copy_threshold_mib: policy.copy_threshold_mib,
        cache_retention_days: policy.cache_retention_days,
        cache_max_mib: policy.cache_max_mib,
        vault_id,
        device_id,
        webdav_url: get(setting_keys::WEBDAV_URL).unwrap_or_default(),
        webdav_username: get(setting_keys::WEBDAV_USERNAME).unwrap_or_default(),
        scan_interval_minutes: get(setting_keys::SCAN_INTERVAL_MINUTES)
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
        schedule_enabled: get(setting_keys::SCHEDULE_ENABLED)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false),
        collect_dirs,
    })
}

/// 保存全部应用设置，并按开关注册/注销 Windows 计划任务
#[tauri::command]
pub async fn set_app_settings(
    settings: AppSettingsDto,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;

    chatvault_metadata::validate_id(&settings.vault_id).map_err(|e| e.to_string())?;
    let current_device = db.ensure_device_identity().map_err(|e| e.to_string())?;
    if current_device != settings.device_id {
        return Err("设备身份由系统生成，不能修改".into());
    }
    let normalized_url = if settings.webdav_url.trim().is_empty() {
        String::new()
    } else {
        WebDavClient::new(WebDavConfig {
            base_url: settings.webdav_url.clone(),
            username: None,
            password: None,
        })
        .map_err(|e| e.to_string())?
        .storage_identity()
        .to_string()
    };
    db.check_remote_binding(&normalized_url, &settings.vault_id)
        .map_err(|e| e.to_string())?;
    // 系统任务注册成功后才保存开关，失败时设置保持原样。
    if settings.schedule_enabled {
        let cli = crate::schedule::find_cli_path()
            .ok_or_else(|| "未找到 chatvault-cli.exe，请将其放在桌面程序同目录".to_string())?;
        crate::schedule::register_scheduled_task(
            &cli,
            &state.db_path,
            settings.scan_interval_minutes,
        )?;
    } else {
        crate::schedule::unregister_scheduled_task()?;
    }
    db.atomic(|db| {
        db.connection()
            .execute(
                "UPDATE app_settings SET value=value WHERE key='remote_binding'",
                [],
            )
            .map_err(|e| chatvault_core::error::ChatVaultError::Database(e.to_string()))?;
        db.check_remote_binding(&normalized_url, &settings.vault_id)?;
        db.set_setting(setting_keys::VAULT_ID, &settings.vault_id)?;
        db.set_setting(setting_keys::DEVICE_ID, &settings.device_id)?;
        db.set_setting(setting_keys::WEBDAV_URL, &normalized_url)?;
        db.set_setting(setting_keys::WEBDAV_USERNAME, &settings.webdav_username)?;
        db.set_setting(
            setting_keys::SCAN_INTERVAL_MINUTES,
            &settings.scan_interval_minutes.to_string(),
        )?;
        db.set_setting(
            setting_keys::SCHEDULE_ENABLED,
            if settings.schedule_enabled {
                "true"
            } else {
                "false"
            },
        )?;

        db.set_setting(COPY_THRESHOLD_MIB, &settings.copy_threshold_mib.to_string())?;
        db.set_setting(
            CACHE_RETENTION_DAYS,
            &settings.cache_retention_days.to_string(),
        )?;
        db.set_setting(CACHE_MAX_MIB, &settings.cache_max_mib.to_string())?;
        let dirs_json = serde_json::to_string(&settings.collect_dirs)?;
        db.set_setting(setting_keys::COLLECT_DIRS, &dirs_json)?;

        Ok(())
    })
    .map_err(|e| e.to_string())?;

    db.reclaim_cache()
        .map_err(|e| format!("设置已保存，但缓存回收失败：{e}"))?;
    Ok(())
}

/// 查询计划任务当前状态
#[tauri::command]
pub async fn get_schedule_status() -> bool {
    crate::schedule::scheduled_task_exists()
}

/// 打开系统目录选择对话框；用户取消时返回 None
#[tauri::command]
pub async fn pick_directory() -> std::result::Result<Option<String>, String> {
    let folder = rfd::AsyncFileDialog::new()
        .set_title("选择采集目录")
        .pick_folder()
        .await;
    Ok(folder.map(|handle| handle.path().to_string_lossy().into_owned()))
}
