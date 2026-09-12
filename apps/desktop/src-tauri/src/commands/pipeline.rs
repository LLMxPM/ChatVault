// ChatVault 桌面命令：与定时任务同构的流水线（扫描 → 归档 → 同步）。
use super::scan::execute_scan;
use super::webdav::resolve_webdav_password;
use super::*;

/// 流水线请求：None 账号表示扫描全部已配置微信账号。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineRequestDto {
    pub target_accounts: Option<Vec<WechatAccountTargetDto>>,
    #[serde(default)]
    pub full_scan: bool,
}

/// 流水线结果：各阶段摘要，便于任务页展示阶段与失败原因。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineResultDto {
    pub webdav_configured: bool,
    pub scan: ScanResultDto,
    pub archive: Option<ArchiveResultDto>,
    pub sync_message: Option<String>,
    pub message: String,
    pub duration_ms: u128,
}

/// 立即运行：与 CLI scheduled-run 同构——扫描、归档、发布并拉取元数据。
#[tauri::command]
pub async fn run_pipeline(
    request: PipelineRequestDto,
    state: State<'_, AppState>,
) -> std::result::Result<PipelineResultDto, String> {
    let start = Instant::now();
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let vault_id = state.vault_id().map_err(|e| e.to_string())?;
    let target = request.target_accounts.as_deref();

    let scan = execute_scan(&mut db, &device_id, target, request.full_scan)?;

    let webdav_url = db
        .get_setting(setting_keys::WEBDAV_URL)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let webdav_username = db
        .get_setting(setting_keys::WEBDAV_USERNAME)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    if webdav_url.trim().is_empty() {
        let new_objects = scan.total_new_objects;
        return Ok(PipelineResultDto {
            webdav_configured: false,
            scan,
            archive: None,
            sync_message: None,
            message: format!("本地扫描完成：新增 {new_objects}。未配置 WebDAV，已跳过归档与同步。"),
            duration_ms: start.elapsed().as_millis(),
        });
    }

    let password = resolve_webdav_password(&webdav_url, &webdav_username, None);
    let cfg = WebDavConfig {
        base_url: webdav_url.clone(),
        username: Some(webdav_username.clone()),
        password,
    };
    let client = WebDavClient::new(cfg).map_err(|e| e.to_string())?;

    let report =
        chatvault_sync::archive::archive_pending(&client, &mut db, &vault_id, &device_id, 0)
            .await
            .map_err(|e| e.to_string())?;
    let archive = ArchiveResultDto {
        uploaded_count: report.uploaded,
        verified_count: report.verified,
        failed_count: report.failed,
        duration_ms: 0,
    };

    let published = chatvault_sync::publish_pending_events(&client, &mut db, &vault_id, &device_id)
        .await
        .map_err(|e| e.to_string())?;
    let applied = chatvault_sync::pull_and_apply(&client, &mut db, &vault_id, &device_id)
        .await
        .map_err(|e| e.to_string())?;
    let sync_message = format!("已发布 seq={published}，应用远端事件 {applied} 条");

    let message = format!(
        "流水线完成：扫描新增 {}，归档成功 {} / 失败 {}。{}",
        scan.total_new_objects, archive.uploaded_count, archive.failed_count, sync_message
    );

    Ok(PipelineResultDto {
        webdav_configured: true,
        scan,
        archive: Some(archive),
        sync_message: Some(sync_message),
        message,
        duration_ms: start.elapsed().as_millis(),
    })
}
