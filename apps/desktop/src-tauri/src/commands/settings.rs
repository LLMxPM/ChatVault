// ChatVault 桌面命令：settings 职责实现与前端错误映射。
use super::*;
use chatvault_core::models::{
    CollectSource, GENERIC_FOLDER_SOURCE_TYPE, WECHAT_WINDOWS_4_SOURCE_TYPE,
};
use chatvault_core::{is_under_root, normalize_scan_key};
use chatvault_index::cache_policy::{CACHE_MAX_MIB, CACHE_RETENTION_DAYS, COPY_THRESHOLD_MIB};

/// 应用设置 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDto {
    pub vault_id: String,
    pub device_id: String,
    /// 可配置设备名称；用于设置页展示与远端设备注册 display_name
    pub device_name: String,
    pub webdav_url: String,
    pub webdav_username: String,
    /// 已关联网盘的 Vault ID；未关联时为 null
    pub bound_vault_id: Option<String>,
    /// 已关联网盘的存储地址；未关联时为 null
    pub bound_webdav_url: Option<String>,
    pub scan_interval_minutes: u32,
    pub schedule_enabled: bool,
    pub copy_threshold_mib: u32,
    pub cache_retention_days: u32,
    pub cache_max_mib: u32,
    /// 远端文件下载目录；空表示默认 Downloads/ChatVault
    pub download_dir: String,
    pub collect_sources: Vec<CollectSource>,
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
    let device_name = state.device_name().map_err(|e| e.to_string())?;
    let (bound_webdav_url, bound_vault_id) =
        match db.get_remote_binding().map_err(|e| e.to_string())? {
            Some((url, vault)) => (Some(url), Some(vault)),
            None => (None, None),
        };

    let collect_sources_raw =
        get(setting_keys::COLLECT_SOURCES).unwrap_or_else(|| "[]".to_string());
    let collect_sources: Vec<CollectSource> =
        serde_json::from_str(&collect_sources_raw).unwrap_or_default();

    let policy = db.cache_policy().map_err(|e| e.to_string())?;
    Ok(AppSettingsDto {
        copy_threshold_mib: policy.copy_threshold_mib,
        cache_retention_days: policy.cache_retention_days,
        cache_max_mib: policy.cache_max_mib,
        download_dir: get(setting_keys::DOWNLOAD_DIR).unwrap_or_default(),
        vault_id,
        device_id,
        device_name,
        bound_vault_id,
        bound_webdav_url,
        webdav_url: get(setting_keys::WEBDAV_URL).unwrap_or_default(),
        webdav_username: get(setting_keys::WEBDAV_USERNAME).unwrap_or_default(),
        scan_interval_minutes: get(setting_keys::SCAN_INTERVAL_MINUTES)
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
        schedule_enabled: get(setting_keys::SCHEDULE_ENABLED)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false),
        collect_sources,
    })
}

/// 保存设置页基础项（身份/连接/缓存）；调度与采集目录由独立命令维护。
#[tauri::command]
pub async fn set_app_settings(
    settings: AppSettingsDto,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;

    chatvault_metadata::validate_vault_id(&settings.vault_id).map_err(|e| e.to_string())?;
    let current_device = db.ensure_device_identity().map_err(|e| e.to_string())?;
    if current_device != settings.device_id {
        return Err("设备身份由系统生成，不能修改".into());
    }
    let device_name = settings.device_name.trim();
    if device_name.is_empty() {
        return Err("设备名称不能为空".into());
    }
    if device_name.len() > 64 {
        return Err("设备名称最长 64 个字符".into());
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
        db.set_setting(setting_keys::DEVICE_NAME, device_name)?;
        db.set_setting(setting_keys::WEBDAV_URL, &normalized_url)?;
        db.set_setting(setting_keys::WEBDAV_USERNAME, &settings.webdav_username)?;

        db.set_setting(COPY_THRESHOLD_MIB, &settings.copy_threshold_mib.to_string())?;
        db.set_setting(
            CACHE_RETENTION_DAYS,
            &settings.cache_retention_days.to_string(),
        )?;
        db.set_setting(CACHE_MAX_MIB, &settings.cache_max_mib.to_string())?;
        db.set_setting(setting_keys::DOWNLOAD_DIR, settings.download_dir.trim())?;

        Ok(())
    })
    .map_err(|e| e.to_string())?;

    db.reclaim_cache()
        .map_err(|e| format!("设置已保存，但缓存回收失败：{e}"))?;
    Ok(())
}

/// 清空旧 Vault 绑定与同步状态，允许改绑新 Vault。
///
/// 保留本地文件索引与来源映射；清除远端绑定、同步游标、日志事件、
/// 已应用事件与设备注册，并将归档任务重新排队。
#[tauri::command]
pub async fn reset_vault_binding(
    state: State<'_, AppState>,
) -> std::result::Result<chatvault_index::VaultResetReport, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    db.reset_vault_binding().map_err(|e| e.to_string())
}

/// 规范化采集源路径；存在的目录使用系统解析后的绝对路径，失效目录保留原路径。
fn normalize_collect_source_path(path: &str) -> String {
    let trimmed = path.trim();
    std::fs::canonicalize(trimmed)
        .unwrap_or_else(|_| PathBuf::from(trimmed))
        .to_string_lossy()
        .into_owned()
}

/// 校验并规范化采集源列表。
///
/// 职责: 校验来源类型、微信根目录约束、路径重复和父子目录重叠；失效路径允许
/// 保留，以便用户在目录被移动后重新选择或移除。
fn validate_collect_sources(
    sources: Vec<CollectSource>,
) -> std::result::Result<Vec<CollectSource>, String> {
    let mut normalized = Vec::with_capacity(sources.len());
    for mut source in sources {
        source.path = normalize_collect_source_path(&source.path);
        if source.path.is_empty() {
            return Err("采集目录不能为空".to_string());
        }
        match source.source_type.as_str() {
            WECHAT_WINDOWS_4_SOURCE_TYPE => {
                let is_xwechat_root = Path::new(&source.path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.eq_ignore_ascii_case("xwechat_files"))
                    .unwrap_or(false);
                if !is_xwechat_root {
                    return Err("微信 4.x 采集源必须是 xwechat_files 根目录".to_string());
                }
                if Path::new(&source.path).exists() {
                    WeChat4Detector::validate_root(&source.path).map_err(|e| e.to_string())?;
                }
            }
            GENERIC_FOLDER_SOURCE_TYPE => {}
            _ => return Err(format!("不支持的采集源类型: {}", source.source_type)),
        }

        let source_key = normalize_scan_key(&source.path);
        if normalized
            .iter()
            .any(|existing: &CollectSource| normalize_scan_key(&existing.path) == source_key)
        {
            return Err("采集目录已存在".to_string());
        }
        if normalized.iter().any(|existing: &CollectSource| {
            is_under_root(&source.path, &existing.path)
                || is_under_root(&existing.path, &source.path)
        }) {
            return Err("采集目录存在父子目录重叠，请只保留较上层目录".to_string());
        }
        normalized.push(source);
    }
    Ok(normalized)
}

/// 持久化带适配器类型的采集源列表（任务页维护）。
#[tauri::command]
pub async fn set_collect_sources(
    sources: Vec<CollectSource>,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let sources = validate_collect_sources(sources)?;
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let sources_json = serde_json::to_string(&sources).map_err(|e| e.to_string())?;
    db.set_setting(setting_keys::COLLECT_SOURCES, &sources_json)
        .map_err(|e| e.to_string())
}

/// 采集源上次探测快照：供任务页首屏直接渲染，避免每次进页全量重扫。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectSourceCacheDto {
    /// 与 collect_sources 中 path 对齐的绝对路径
    pub path: String,
    /// checking / ready / empty / missing / error
    pub status: String,
    #[serde(default)]
    pub error_message: String,
    #[serde(default)]
    pub accounts: Vec<WechatAccountDto>,
    /// 探测完成时间（Unix 毫秒）
    #[serde(default)]
    pub inspected_at: i64,
}

/// 读取采集源探测快照。
#[tauri::command]
pub async fn get_collect_source_cache(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<CollectSourceCacheDto>, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let raw = db
        .get_setting(setting_keys::COLLECT_SOURCE_CACHE)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "[]".to_string());
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// 写入采集源探测快照；仅缓存 UI 展示，不参与扫描编排。
#[tauri::command]
pub async fn set_collect_source_cache(
    cache: Vec<CollectSourceCacheDto>,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let raw = serde_json::to_string(&cache).map_err(|e| e.to_string())?;
    db.set_setting(setting_keys::COLLECT_SOURCE_CACHE, &raw)
        .map_err(|e| e.to_string())
}

/// 读取立即运行勾选的微信账号；None 表示从未配置（前端默认全选）。
#[tauri::command]
pub async fn get_collect_selected_accounts(
    state: State<'_, AppState>,
) -> std::result::Result<Option<Vec<WechatAccountTargetDto>>, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let Some(raw) = db
        .get_setting(setting_keys::COLLECT_SELECTED_ACCOUNTS)
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// 持久化立即运行勾选的微信账号；空数组表示用户明确取消全部。
#[tauri::command]
pub async fn set_collect_selected_accounts(
    accounts: Vec<WechatAccountTargetDto>,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let raw = serde_json::to_string(&accounts).map_err(|e| e.to_string())?;
    db.set_setting(setting_keys::COLLECT_SELECTED_ACCOUNTS, &raw)
        .map_err(|e| e.to_string())
}

/// 更新定时扫描开关与周期，并注册/注销系统计划任务。
#[tauri::command]
pub async fn set_schedule_config(
    enabled: bool,
    interval_minutes: u32,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    if enabled {
        let interval = interval_minutes.clamp(5, crate::schedule::MAX_INTERVAL_MINUTES);
        let cli = crate::schedule::find_cli_path()
            .ok_or_else(|| "未找到 chatvault-cli.exe，请将其放在桌面程序同目录".to_string())?;
        crate::schedule::register_scheduled_task(&cli, &state.db_path, interval)?;
        let mut db = state.get_db().map_err(|e| e.to_string())?;
        db.set_setting(setting_keys::SCAN_INTERVAL_MINUTES, &interval.to_string())
            .map_err(|e| e.to_string())?;
        db.set_setting(setting_keys::SCHEDULE_ENABLED, "true")
            .map_err(|e| e.to_string())?;
    } else {
        crate::schedule::unregister_scheduled_task()?;
        let mut db = state.get_db().map_err(|e| e.to_string())?;
        db.set_setting(setting_keys::SCHEDULE_ENABLED, "false")
            .map_err(|e| e.to_string())?;
        if interval_minutes > 0 {
            let interval = interval_minutes.clamp(5, crate::schedule::MAX_INTERVAL_MINUTES);
            db.set_setting(setting_keys::SCAN_INTERVAL_MINUTES, &interval.to_string())
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 查询计划任务当前状态
#[tauri::command]
pub async fn get_schedule_status() -> bool {
    crate::schedule::scheduled_task_exists()
}

/// 打开系统目录选择对话框；用户取消时返回 None
#[tauri::command]
pub async fn pick_directory(title: Option<String>) -> std::result::Result<Option<String>, String> {
    let folder = rfd::AsyncFileDialog::new()
        .set_title(title.as_deref().unwrap_or("选择目录"))
        .pick_folder()
        .await;
    Ok(folder.map(|handle| handle.path().to_string_lossy().into_owned()))
}

/// 检查采集源目录当前是否存在且仍为可访问目录。
#[tauri::command]
pub async fn check_directory(path: String) -> bool {
    let path = Path::new(path.trim());
    path.is_dir() && std::fs::read_dir(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(source_type: &str, path: &Path) -> CollectSource {
        CollectSource {
            source_type: source_type.to_string(),
            path: path.to_string_lossy().into_owned(),
            enable_videos: true,
        }
    }

    #[test]
    fn collect_sources_reject_invalid_type_duplicate_and_overlap() {
        let base = std::env::temp_dir().join(format!(
            "chatvault-collect-sources-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let wechat = base.join("xwechat_files");
        let attachments = base.join("attachments");
        let nested = attachments.join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        assert!(validate_collect_sources(vec![
            source(WECHAT_WINDOWS_4_SOURCE_TYPE, &attachments,)
        ])
        .is_err());
        assert!(validate_collect_sources(vec![
            source(GENERIC_FOLDER_SOURCE_TYPE, &attachments),
            source(GENERIC_FOLDER_SOURCE_TYPE, &nested),
        ])
        .is_err());
        assert!(validate_collect_sources(vec![
            source(GENERIC_FOLDER_SOURCE_TYPE, &attachments),
            source(GENERIC_FOLDER_SOURCE_TYPE, &attachments),
        ])
        .is_err());
        assert!(validate_collect_sources(vec![
            source(WECHAT_WINDOWS_4_SOURCE_TYPE, &wechat),
            source(GENERIC_FOLDER_SOURCE_TYPE, &attachments),
        ])
        .is_ok());

        std::fs::remove_dir_all(base).unwrap();
    }
}
