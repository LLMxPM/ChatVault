// 定时流水线：复用采集与归档服务，将全部退出路径写入运行历史。
mod reporting;
mod scan;

use anyhow::{Context, Result};
use chatvault_core::models::{TaskRunKind, TaskRunStageName, TaskRunStageStatus, TaskRunStatus};
use chatvault_index::Database;
use chatvault_sync::{archive_pending_with_progress, NoopProgressSink};
use chatvault_webdav::{WebDavClient, WebDavConfig};
use reporting::{archive_stats, finish_run, scan_stats};
use serde_json::json;
use std::path::Path;
use tracing::Instrument;

/// 创建运行记录后执行流水线；配置、扫描及网络错误均收尾，避免残留 running。
pub(crate) async fn run(db_path: &Path) -> Result<()> {
    tracing::info!(executable = %std::env::current_exe()?.display(), database = %db_path.display(), "开始定时运行");
    let mut db = Database::open(db_path)?;
    db.maintain_task_runs_on_startup()?;
    let run_id = db.start_task_run(TaskRunKind::Pipeline, "schedule", false, "cli")?;
    let span = tracing::info_span!("scheduled_run", run_id = %run_id);
    let result = execute(&mut db, &run_id).instrument(span.clone()).await;
    if let Err(error) = &result {
        let _guard = span.enter();
        if let Err(save_error) = reporting::finish_error(&mut db, &run_id, error) {
            tracing::error!(%save_error, "保存定时运行失败记录失败");
        }
    }
    result
}

/// 按扫描、归档、发布、拉取顺序运行；阶段统计是最终摘要的唯一来源。
async fn execute(db: &mut Database, run_id: &str) -> Result<()> {
    db.start_task_run_stage(run_id, TaskRunStageName::Scan)?;
    let device_id = db.ensure_device_identity()?;
    let vault_id = db
        .get_setting("vault_id")?
        .unwrap_or_else(|| "chatvault-default".to_string());
    let webdav_url = db.get_setting("webdav_url")?.unwrap_or_default();
    let webdav_configured = !webdav_url.trim().is_empty();
    db.set_task_run_webdav_configured(run_id, webdav_configured)?;
    let report = scan::collect(db, run_id, &device_id)?;
    let stats = scan_stats(&report);
    if db.task_run_cancel_requested(run_id)? {
        return reporting::finish_cancelled(db, run_id, TaskRunStageName::Scan, &stats);
    }
    let message = format!(
        "候选 {} · 新文件 {} · 跳过 {}",
        report.discovered, report.new_objects, report.skipped
    );
    db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Scan,
        TaskRunStageStatus::Success,
        Some(&stats.to_string()),
        Some(&message),
    )?;
    tracing::info!("扫描完成：{message}");

    if !webdav_configured {
        db.start_task_run_stage(run_id, TaskRunStageName::Archive)?;
        db.finish_task_run_stage(
            run_id,
            TaskRunStageName::Archive,
            TaskRunStageStatus::Skipped,
            None,
            Some("未配置 WebDAV"),
        )?;
        tracing::info!("未配置 WebDAV，跳过归档与同步");
        return finish_run(db, run_id, TaskRunStatus::Success, None);
    }

    db.start_task_run_stage(run_id, TaskRunStageName::Archive)?;
    tracing::info!("开始归档");
    let webdav_user = db.get_setting("webdav_username")?.unwrap_or_default();
    let key = format!(
        "{}:{}",
        webdav_url.trim().trim_end_matches('/'),
        webdav_user.trim()
    );
    let password = keyring::Entry::new("chatvault-webdav", &key)
        .and_then(|entry| entry.get_password())
        .ok();
    let client = WebDavClient::new(WebDavConfig {
        base_url: webdav_url,
        username: Some(webdav_user),
        password,
    })
    .context("创建 WebDAV 客户端失败")?;
    let archive = archive_pending_with_progress(
        &client,
        db,
        &vault_id,
        &device_id,
        0,
        &mut NoopProgressSink,
        Some(run_id),
    )
    .await
    .context("归档失败")?;
    let stats = archive_stats(&archive);
    if archive.cancelled {
        return reporting::finish_cancelled(db, run_id, TaskRunStageName::Archive, &stats);
    }
    let message = format!("上传 {} · 失败 {}", archive.uploaded, archive.failed);
    db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Archive,
        TaskRunStageStatus::Success,
        Some(&stats.to_string()),
        Some(&message),
    )?;
    tracing::info!("归档完成：{message}");

    db.start_task_run_stage(run_id, TaskRunStageName::Publish)?;
    tracing::info!("开始发布元数据");
    let published = chatvault_sync::publish_pending_events(&client, db, &vault_id, &device_id)
        .await
        .context("发布元数据失败")?;
    db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Publish,
        TaskRunStageStatus::Success,
        Some(&json!({ "publishedSeq": published }).to_string()),
        Some(&format!("已发布 seq={published}")),
    )?;
    tracing::info!(published, "元数据发布完成");

    db.start_task_run_stage(run_id, TaskRunStageName::Pull)?;
    tracing::info!("开始拉取远端元数据");
    let applied = chatvault_sync::pull_and_apply(&client, db, &vault_id, &device_id)
        .await
        .context("拉取远端元数据失败")?;
    db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Pull,
        TaskRunStageStatus::Success,
        Some(&json!({ "applied": applied }).to_string()),
        Some(&format!("应用远端事件 {applied} 条")),
    )?;
    tracing::info!(applied, "远端元数据拉取完成");

    let failed_items = reporting::failed_items(db, run_id)?;
    let status = if archive.failed > 0 || failed_items > 0 {
        TaskRunStatus::Partial
    } else {
        TaskRunStatus::Success
    };
    finish_run(db, run_id, status, None)?;
    if archive.failed > 0 || failed_items > 0 {
        anyhow::bail!("存在归档异常，已保存失败原因并等待处理");
    }
    Ok(())
}
