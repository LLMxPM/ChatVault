// ChatVault 桌面命令：与定时任务同构的流水线（扫描 → 归档 → 同步），并写入运行日志。
use super::scan::execute_scan;
use super::webdav::resolve_webdav_password;
use super::*;
use chatvault_core::models::{TaskRunKind, TaskRunStageName, TaskRunStageStatus, TaskRunStatus};
use chatvault_index::{item_from_row, stage_from_row, TaskRunRow};
use chatvault_sync::{archive_pending_with_progress, ArchiveProgressSink};
use serde_json::json;
use tauri::{AppHandle, Emitter};

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
    pub run_id: String,
    pub webdav_configured: bool,
    pub scan: ScanResultDto,
    pub archive: Option<ArchiveResultDto>,
    pub sync_message: Option<String>,
    pub message: String,
    pub duration_ms: u128,
    pub status: String,
}

/// 运行列表项 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRunDto {
    pub run_id: String,
    pub kind: String,
    pub trigger_source: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub webdav_configured: bool,
    pub summary_json: Option<String>,
    pub error_message: Option<String>,
    pub failed_items: i64,
    pub runner_kind: String,
    pub cancel_requested: bool,
    pub heartbeat_at: Option<String>,
}

/// 阶段 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRunStageDto {
    pub stage: String,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub stats_json: Option<String>,
    pub message: Option<String>,
}

/// 明细 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRunItemDto {
    pub item_id: String,
    pub stage: String,
    pub record_id: Option<String>,
    pub task_id: Option<String>,
    pub name: String,
    pub status: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub size: Option<i64>,
    pub updated_at: String,
}

/// 运行详情
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRunDetailDto {
    pub run: TaskRunDto,
    pub stages: Vec<TaskRunStageDto>,
    pub items: Vec<TaskRunItemDto>,
}

/// 将任务运行阶段进度推送到前端。
struct TauriProgressSink {
    app: AppHandle,
    run_id: String,
}

impl TauriProgressSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.app.emit(event, payload);
    }
}

impl ArchiveProgressSink for TauriProgressSink {
    fn on_stage(&mut self, stage: &str, status: &str, message: Option<&str>) {
        self.emit(
            "run://stage",
            json!({
                "runId": self.run_id,
                "stage": stage,
                "status": status,
                "message": message,
            }),
        );
    }

    fn on_progress(&mut self, stage: &str, done: usize, total: usize, current: Option<&str>) {
        self.emit(
            "run://progress",
            json!({
                "runId": self.run_id,
                "stage": stage,
                "done": done,
                "total": total,
                "currentName": current,
            }),
        );
    }

    fn on_item(
        &mut self,
        stage: &str,
        task_id: Option<&str>,
        record_id: Option<&str>,
        name: &str,
        status: &str,
        error: Option<&str>,
        size: Option<u64>,
    ) {
        self.emit(
            "run://item",
            json!({
                "runId": self.run_id,
                "stage": stage,
                "taskId": task_id,
                "recordId": record_id,
                "name": name,
                "status": status,
                "error": error,
                "size": size,
            }),
        );
    }
}

fn run_row_to_dto(db: &chatvault_index::Database, row: &TaskRunRow) -> Result<TaskRunDto, String> {
    let counts = db
        .count_task_run_items_by_status(&row.run_id)
        .map_err(|e| e.to_string())?;
    let failed_items: i64 = counts
        .iter()
        .filter(|(s, _)| s != "skipped")
        .map(|(_, c)| *c)
        .sum();
    Ok(TaskRunDto {
        run_id: row.run_id.clone(),
        kind: row.kind.clone(),
        trigger_source: row.trigger_source.clone(),
        status: row.status.clone(),
        started_at: row.started_at.clone(),
        finished_at: row.finished_at.clone(),
        duration_ms: row.duration_ms,
        webdav_configured: row.webdav_configured,
        summary_json: row.summary_json.clone(),
        error_message: row.error_message.clone(),
        failed_items,
        runner_kind: row.runner_kind.clone(),
        cancel_requested: row.cancel_requested,
        heartbeat_at: row.heartbeat_at.clone(),
    })
}

/// 列出最近任务运行
#[tauri::command]
pub async fn list_task_runs(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<TaskRunDto>, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let rows = db
        .list_task_runs(limit.unwrap_or(50))
        .map_err(|e| e.to_string())?;
    rows.iter().map(|row| run_row_to_dto(&db, row)).collect()
}

/// 读取运行详情（阶段 + 明细）
#[tauri::command]
pub async fn get_task_run_detail(
    run_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<TaskRunDetailDto, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let row = db
        .get_task_run(&run_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "运行记录不存在".to_string())?;
    let stages = db
        .list_task_run_stages(&run_id)
        .map_err(|e| e.to_string())?;
    let items = db
        .list_task_run_items(&run_id, None, None, 500)
        .map_err(|e| e.to_string())?;

    let mut stage_dtos = Vec::with_capacity(stages.len());
    for s in &stages {
        let stage = stage_from_row(s).map_err(|e| e.to_string())?;
        stage_dtos.push(TaskRunStageDto {
            stage: stage.stage.as_str().to_string(),
            status: stage.status.as_str().to_string(),
            started_at: s.started_at.clone(),
            finished_at: s.finished_at.clone(),
            duration_ms: s.duration_ms,
            stats_json: s.stats_json.clone(),
            message: s.message.clone(),
        });
    }
    let mut item_dtos = Vec::with_capacity(items.len());
    for i in &items {
        let item = item_from_row(i).map_err(|e| e.to_string())?;
        item_dtos.push(TaskRunItemDto {
            item_id: item.item_id.clone(),
            stage: item.stage.as_str().to_string(),
            record_id: item.record_id.clone(),
            task_id: item.task_id.clone(),
            name: item.name.clone(),
            status: item.status.as_str().to_string(),
            error_code: item.error_code.clone(),
            error_message: item.error_message.clone(),
            size: item.size,
            updated_at: i.updated_at.clone(),
        });
    }

    Ok(TaskRunDetailDto {
        run: run_row_to_dto(&db, &row)?,
        stages: stage_dtos,
        items: item_dtos,
    })
}

/// 立即运行：与 CLI scheduled-run 同构——扫描、归档、发布并拉取元数据，并记录运行日志。
#[tauri::command]
pub async fn run_pipeline(
    app: AppHandle,
    request: PipelineRequestDto,
    state: State<'_, AppState>,
) -> std::result::Result<PipelineResultDto, String> {
    let start = Instant::now();
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let vault_id = state.vault_id().map_err(|e| e.to_string())?;
    let target = request.target_accounts.as_deref();

    let webdav_url = db
        .get_setting(setting_keys::WEBDAV_URL)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let webdav_configured = !webdav_url.trim().is_empty();

    let run_id = db
        .start_task_run(
            TaskRunKind::Pipeline,
            "manual",
            webdav_configured,
            "desktop",
        )
        .map_err(|e| e.to_string())?;
    let mut sink = TauriProgressSink {
        app: app.clone(),
        run_id: run_id.clone(),
    };
    sink.emit(
        "run://started",
        json!({ "runId": run_id, "trigger": "manual", "kind": "pipeline" }),
    );

    // 扫描阶段
    db.start_task_run_stage(&run_id, TaskRunStageName::Scan)
        .map_err(|e| e.to_string())?;
    sink.on_stage("scan", "running", Some("扫描中"));
    if db
        .task_run_cancel_requested(&run_id)
        .map_err(|e| e.to_string())?
    {
        return finish_cancelled_pipeline(
            &mut db,
            &mut sink,
            &run_id,
            TaskRunStageName::Scan,
            None,
            "扫描前取消",
        );
    }
    let scan = match execute_scan(&mut db, &device_id, target, request.full_scan) {
        Ok(s) => s,
        Err(e) => {
            let _ = db.finish_task_run_stage(
                &run_id,
                TaskRunStageName::Scan,
                TaskRunStageStatus::Failed,
                None,
                Some(&e),
            );
            let _ = db.finish_task_run(
                &run_id,
                TaskRunStatus::Failed,
                None,
                Some(&format!("扫描失败：{e}")),
            );
            sink.emit(
                "run://finished",
                json!({ "runId": run_id, "status": "failed", "message": e }),
            );
            return Err(e);
        }
    };
    let scan_stats = json!({
        "discovered": scan.total_discovered,
        "newObjects": scan.total_new_objects,
        "skipped": scan.total_skipped,
    });
    db.finish_task_run_stage(
        &run_id,
        TaskRunStageName::Scan,
        TaskRunStageStatus::Success,
        Some(&scan_stats.to_string()),
        Some(&format!(
            "发现 {} · 新增 {} · 跳过 {}",
            scan.total_discovered, scan.total_new_objects, scan.total_skipped
        )),
    )
    .map_err(|e| e.to_string())?;
    let _ = db.touch_task_run_progress(
        &run_id,
        TaskRunStageName::Scan,
        scan.total_new_objects,
        scan.total_discovered,
        None,
    );
    sink.on_stage(
        "scan",
        "success",
        Some(&format!(
            "发现 {} · 新增 {}",
            scan.total_discovered, scan.total_new_objects
        )),
    );

    if !webdav_configured {
        let message = format!(
            "本地扫描完成：新增 {}。未配置 WebDAV，已跳过归档与同步。",
            scan.total_new_objects
        );
        db.finish_task_run_stage(
            &run_id,
            TaskRunStageName::Archive,
            TaskRunStageStatus::Skipped,
            None,
            Some("未配置 WebDAV"),
        )
        .map_err(|e| e.to_string())?;
        let summary = json!({
            "scan": scan_stats,
            "archive": null,
            "sync": null,
        });
        db.finish_task_run(
            &run_id,
            TaskRunStatus::Success,
            Some(&summary.to_string()),
            None,
        )
        .map_err(|e| e.to_string())?;
        sink.emit(
            "run://finished",
            json!({ "runId": run_id, "status": "success", "message": message }),
        );
        return Ok(PipelineResultDto {
            run_id,
            webdav_configured: false,
            scan,
            archive: None,
            sync_message: None,
            message,
            duration_ms: start.elapsed().as_millis(),
            status: "success".into(),
        });
    }

    let username = webdav_username_from_db(&db)?;
    let password = resolve_webdav_password(&webdav_url, &username, None);
    let cfg = WebDavConfig {
        base_url: webdav_url.clone(),
        username: Some(username),
        password,
    };
    let client = WebDavClient::new(cfg).map_err(|e| e.to_string())?;

    // 归档阶段
    db.start_task_run_stage(&run_id, TaskRunStageName::Archive)
        .map_err(|e| e.to_string())?;
    sink.on_stage("archive", "running", Some("归档上传中"));
    let report = match archive_pending_with_progress(
        &client,
        &mut db,
        &vault_id,
        &device_id,
        0,
        &mut sink,
        Some(&run_id),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            let _ = db.finish_task_run_stage(
                &run_id,
                TaskRunStageName::Archive,
                TaskRunStageStatus::Failed,
                None,
                Some(&msg),
            );
            let _ = db.finish_task_run(
                &run_id,
                TaskRunStatus::Failed,
                None,
                Some(&format!("归档失败：{msg}")),
            );
            sink.emit(
                "run://finished",
                json!({ "runId": run_id, "status": "failed", "message": msg }),
            );
            return Err(msg);
        }
    };
    if report.cancelled {
        let message = format!(
            "已取消。扫描新增 {}，归档成功 {} / 失败 {}。",
            scan.total_new_objects, report.uploaded, report.failed
        );
        return finish_cancelled_after_archive(
            &mut db,
            &mut sink,
            &run_id,
            &scan_stats,
            &report,
            &message,
        );
    }
    let archive = ArchiveResultDto {
        uploaded_count: report.uploaded,
        verified_count: report.verified,
        failed_count: report.failed,
        duration_ms: 0,
    };
    let archive_stats = json!({
        "uploaded": archive.uploaded_count,
        "verified": archive.verified_count,
        "failed": archive.failed_count,
        "processed": report.processed,
    });
    db.finish_task_run_stage(
        &run_id,
        TaskRunStageName::Archive,
        TaskRunStageStatus::Success,
        Some(&archive_stats.to_string()),
        Some(&format!(
            "上传 {} · 失败 {}",
            archive.uploaded_count, archive.failed_count
        )),
    )
    .map_err(|e| e.to_string())?;

    // 同步阶段：发布 + 拉取
    db.start_task_run_stage(&run_id, TaskRunStageName::Publish)
        .map_err(|e| e.to_string())?;
    sink.on_stage("publish", "running", Some("发布元数据"));
    let published =
        match chatvault_sync::publish_pending_events(&client, &mut db, &vault_id, &device_id).await
        {
            Ok(p) => p,
            Err(e) => {
                let msg = e.to_string();
                let _ = db.finish_task_run_stage(
                    &run_id,
                    TaskRunStageName::Publish,
                    TaskRunStageStatus::Failed,
                    None,
                    Some(&msg),
                );
                let _ = db.finish_task_run(
                    &run_id,
                    TaskRunStatus::Partial,
                    Some(
                        &json!({
                            "scan": scan_stats,
                            "archive": archive_stats,
                            "error": msg,
                        })
                        .to_string(),
                    ),
                    Some(&format!("发布失败：{msg}")),
                );
                sink.emit(
                    "run://finished",
                    json!({ "runId": run_id, "status": "partial", "message": msg }),
                );
                let message = format!(
                    "扫描新增 {}，归档成功 {} / 失败 {}。发布失败：{msg}",
                    scan.total_new_objects, archive.uploaded_count, archive.failed_count
                );
                return Ok(PipelineResultDto {
                    run_id,
                    webdav_configured: true,
                    scan,
                    archive: Some(archive),
                    sync_message: Some(msg),
                    message,
                    duration_ms: start.elapsed().as_millis(),
                    status: "partial".into(),
                });
            }
        };
    db.finish_task_run_stage(
        &run_id,
        TaskRunStageName::Publish,
        TaskRunStageStatus::Success,
        Some(&json!({ "publishedSeq": published }).to_string()),
        Some(&format!("已发布 seq={published}")),
    )
    .map_err(|e| e.to_string())?;
    sink.on_stage(
        "publish",
        "success",
        Some(&format!("已发布 seq={published}")),
    );

    db.start_task_run_stage(&run_id, TaskRunStageName::Pull)
        .map_err(|e| e.to_string())?;
    sink.on_stage("pull", "running", Some("拉取远端事件"));
    let applied =
        match chatvault_sync::pull_and_apply(&client, &mut db, &vault_id, &device_id).await {
            Ok(a) => a,
            Err(e) => {
                let msg = e.to_string();
                let _ = db.finish_task_run_stage(
                    &run_id,
                    TaskRunStageName::Pull,
                    TaskRunStageStatus::Failed,
                    None,
                    Some(&msg),
                );
                let _ = db.finish_task_run(
                    &run_id,
                    TaskRunStatus::Partial,
                    Some(
                        &json!({
                            "scan": scan_stats,
                            "archive": archive_stats,
                            "publishedSeq": published,
                            "error": msg,
                        })
                        .to_string(),
                    ),
                    Some(&format!("拉取失败：{msg}")),
                );
                sink.emit(
                    "run://finished",
                    json!({ "runId": run_id, "status": "partial", "message": msg }),
                );
                let message = format!(
                    "扫描新增 {}，归档成功 {} / 失败 {}。已发布 seq={published}，拉取失败：{msg}",
                    scan.total_new_objects, archive.uploaded_count, archive.failed_count
                );
                return Ok(PipelineResultDto {
                    run_id,
                    webdav_configured: true,
                    scan,
                    archive: Some(archive),
                    sync_message: Some(msg),
                    message,
                    duration_ms: start.elapsed().as_millis(),
                    status: "partial".into(),
                });
            }
        };
    let sync_message = format!("已发布 seq={published}，应用远端事件 {applied} 条");
    db.finish_task_run_stage(
        &run_id,
        TaskRunStageName::Pull,
        TaskRunStageStatus::Success,
        Some(&json!({ "applied": applied }).to_string()),
        Some(&sync_message),
    )
    .map_err(|e| e.to_string())?;
    sink.on_stage("pull", "success", Some(&sync_message));

    let item_counts = db
        .count_task_run_items_by_status(&run_id)
        .map_err(|e| e.to_string())?;
    let failed_items: i64 = item_counts
        .iter()
        .filter(|(s, _)| s != "skipped")
        .map(|(_, c)| *c)
        .sum();

    let status = if archive.failed_count > 0 || failed_items > 0 {
        "partial"
    } else {
        "success"
    };
    let status_enum = if status == "partial" {
        TaskRunStatus::Partial
    } else {
        TaskRunStatus::Success
    };

    let summary = json!({
        "scan": scan_stats,
        "archive": archive_stats,
        "publishedSeq": published,
        "applied": applied,
        "failedItems": failed_items,
    });
    db.finish_task_run(&run_id, status_enum, Some(&summary.to_string()), None)
        .map_err(|e| e.to_string())?;

    let message = format!(
        "流水线完成：扫描新增 {}，归档成功 {} / 失败 {}。{}",
        scan.total_new_objects, archive.uploaded_count, archive.failed_count, sync_message
    );
    sink.emit(
        "run://finished",
        json!({ "runId": run_id, "status": status, "message": message }),
    );

    Ok(PipelineResultDto {
        run_id,
        webdav_configured: true,
        scan,
        archive: Some(archive),
        sync_message: Some(sync_message),
        message,
        duration_ms: start.elapsed().as_millis(),
        status: status.into(),
    })
}

fn webdav_username_from_db(db: &chatvault_index::Database) -> std::result::Result<String, String> {
    Ok(db
        .get_setting(setting_keys::WEBDAV_USERNAME)
        .map_err(|e| e.to_string())?
        .unwrap_or_default())
}

/// 扫描前后取消：当前阶段 skipped，整 run cancelled。
fn finish_cancelled_pipeline(
    db: &mut chatvault_index::Database,
    sink: &mut TauriProgressSink,
    run_id: &str,
    stage: TaskRunStageName,
    scan_stats: Option<&serde_json::Value>,
    message: &str,
) -> std::result::Result<PipelineResultDto, String> {
    let _ = db.finish_task_run_stage(
        run_id,
        stage,
        TaskRunStageStatus::Skipped,
        None,
        Some("用户取消"),
    );
    let summary = scan_stats.map(|s| json!({ "scan": s, "cancelled": true }).to_string());
    let _ = db.finish_task_run(
        run_id,
        TaskRunStatus::Cancelled,
        summary.as_deref(),
        Some("用户取消"),
    );
    sink.on_stage(stage.as_str(), "skipped", Some("用户取消"));
    sink.emit(
        "run://finished",
        json!({ "runId": run_id, "status": "cancelled", "message": message }),
    );
    Ok(PipelineResultDto {
        run_id: run_id.to_string(),
        webdav_configured: false,
        scan: Default::default(),
        archive: None,
        sync_message: None,
        message: message.to_string(),
        duration_ms: 0,
        status: "cancelled".into(),
    })
}

/// 归档阶段取消：保留扫描结果。
fn finish_cancelled_after_archive(
    db: &mut chatvault_index::Database,
    sink: &mut TauriProgressSink,
    run_id: &str,
    scan_stats: &serde_json::Value,
    report: &chatvault_sync::ArchiveReport,
    message: &str,
) -> std::result::Result<PipelineResultDto, String> {
    let archive_stats = json!({
        "uploaded": report.uploaded,
        "verified": report.verified,
        "failed": report.failed,
        "processed": report.processed,
        "cancelled": true,
    });
    let _ = db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Archive,
        TaskRunStageStatus::Skipped,
        Some(&archive_stats.to_string()),
        Some("用户取消"),
    );
    let _ = db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Publish,
        TaskRunStageStatus::Skipped,
        None,
        Some("用户取消"),
    );
    let _ = db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Pull,
        TaskRunStageStatus::Skipped,
        None,
        Some("用户取消"),
    );
    let summary = json!({
        "scan": scan_stats,
        "archive": archive_stats,
        "cancelled": true,
    });
    let _ = db.finish_task_run(
        run_id,
        TaskRunStatus::Cancelled,
        Some(&summary.to_string()),
        Some("用户取消"),
    );
    sink.emit(
        "run://finished",
        json!({ "runId": run_id, "status": "cancelled", "message": message }),
    );
    Ok(PipelineResultDto {
        run_id: run_id.to_string(),
        webdav_configured: true,
        scan: Default::default(),
        archive: Some(ArchiveResultDto {
            uploaded_count: report.uploaded,
            verified_count: report.verified,
            failed_count: report.failed,
            duration_ms: 0,
        }),
        sync_message: None,
        message: message.to_string(),
        duration_ms: 0,
        status: "cancelled".into(),
    })
}

/// 当前正在执行的运行（桌面监视器轮询用；含阶段 stats 进度）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRunDto {
    pub run_id: String,
    pub kind: String,
    pub trigger_source: String,
    pub runner_kind: String,
    pub status: String,
    pub started_at: String,
    pub heartbeat_at: Option<String>,
    pub cancel_requested: bool,
    pub current_stage: Option<String>,
    pub stage_status: Option<String>,
    pub done: Option<usize>,
    pub total: Option<usize>,
    pub current_name: Option<String>,
}

/// 读取当前 running 运行及其最新阶段进度
#[tauri::command]
pub async fn get_active_run(
    state: State<'_, AppState>,
) -> std::result::Result<Option<ActiveRunDto>, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let Some(row) = db.get_active_task_run().map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let stages = db
        .list_task_run_stages(&row.run_id)
        .map_err(|e| e.to_string())?;
    let mut current_stage = None;
    let mut stage_status = None;
    let mut done = None;
    let mut total = None;
    let mut current_name = None;
    for s in stages.iter().rev() {
        if s.status == "running" || s.status == "pending" {
            current_stage = Some(s.stage.clone());
            stage_status = Some(s.status.clone());
            if let Some(stats) = s.stats_json.as_deref() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(stats) {
                    done = v.get("done").and_then(|x| x.as_u64()).map(|x| x as usize);
                    total = v.get("total").and_then(|x| x.as_u64()).map(|x| x as usize);
                    current_name = v
                        .get("currentName")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string());
                }
            }
            break;
        }
    }
    Ok(Some(ActiveRunDto {
        run_id: row.run_id,
        kind: row.kind,
        trigger_source: row.trigger_source,
        runner_kind: row.runner_kind,
        status: row.status,
        started_at: row.started_at,
        heartbeat_at: row.heartbeat_at,
        cancel_requested: row.cancel_requested,
        current_stage,
        stage_status,
        done,
        total,
        current_name,
    }))
}

/// 请求取消运行中的流水线（含 CLI 定时运行）
#[tauri::command]
pub async fn cancel_task_run(
    run_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<bool, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    db.request_cancel_task_run(&run_id)
        .map_err(|e| e.to_string())
}
