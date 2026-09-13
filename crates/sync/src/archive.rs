// ChatVault 共享归档队列：桌面与计划任务采用相同重试、暂存和校验规则。
use crate::progress::{ArchiveProgressSink, NoopProgressSink};
use chatvault_core::models::TaskRunItemStatus;
use chatvault_core::{error::Result, models::VaultConfig};
use chatvault_index::{Database, NewTaskRunItem};
use chatvault_webdav::{ObjectPublisher, PublishResult, WebDavClient};

/// 一轮归档结果；失败任务保留错误并按退避策略等待重试。
#[derive(Default)]
pub struct ArchiveReport {
    pub uploaded: usize,
    pub verified: usize,
    pub failed: usize,
    /// 本轮实际尝试的文件数
    pub processed: usize,
}

/// 归档当前可执行任务；limit 为 0 时持续取批次直到当前队列处理完。
pub async fn archive_pending(
    client: &WebDavClient,
    db: &mut Database,
    vault: &str,
    device: &str,
    limit: usize,
) -> Result<ArchiveReport> {
    archive_pending_with_progress(
        client,
        db,
        vault,
        device,
        limit,
        &mut NoopProgressSink,
        None,
    )
    .await
}

/// 带进度回调与运行明细的归档。
/// `run_id` 为 Some 时，失败/缺失项写入 `task_run_items`。
/// 回调不要访问传入的 `db`（其可变借用仍被本函数持有）。
pub async fn archive_pending_with_progress(
    client: &WebDavClient,
    db: &mut Database,
    vault: &str,
    device: &str,
    limit: usize,
    progress: &mut dyn ArchiveProgressSink,
    run_id: Option<&str>,
) -> Result<ArchiveReport> {
    db.reclaim_cache()?;
    db.check_remote_binding(client.storage_identity(), vault)?;
    chatvault_webdav::ensure_vault_config(
        client,
        &VaultConfig {
            vault_id: vault.into(),
            format_version: 1,
            hash_algorithm: "blake3".into(),
            created_at: chrono::Utc::now(),
        },
    )
    .await?;
    db.bind_remote(client.storage_identity(), vault)?;
    progress.on_stage("archive", "running", Some("开始归档上传"));

    let mut report = ArchiveReport::default();
    let mut processed = 0usize;
    // 本轮失败任务不再执行，避免慢速网络使重试时间在同一轮内到期。
    let mut attempted = std::collections::HashSet::new();
    loop {
        let tasks = db.pending_uploads()?;
        let tasks: Vec<_> = tasks
            .into_iter()
            .filter(|t| !attempted.contains(&t.task_id))
            .collect();
        if tasks.is_empty() {
            break;
        }
        let batch_total = processed + tasks.len();
        for task in tasks {
            if limit != 0 && processed >= limit {
                return Ok(report);
            }
            attempted.insert(task.task_id.clone());
            // 用户在批次选取后暂停的任务仍应被跳过。
            if !db.task_is_runnable(&task.task_id)? {
                continue;
            }
            processed += 1;
            report.processed = processed;
            progress.on_progress(
                "archive",
                processed,
                batch_total.max(processed),
                Some(&task.original_name),
            );

            let path = match db.upload_source(&task.task_id) {
                Ok(path) => path,
                Err(e) => {
                    report.failed += 1;
                    db.update_task_status(&task.task_id, "missing", Some(&e.to_string()))?;
                    record_archive_item(
                        db,
                        run_id,
                        &task,
                        TaskRunItemStatus::Missing,
                        Some(&e.to_string()),
                    )?;
                    progress.on_item(
                        "archive",
                        Some(&task.task_id),
                        Some(&task.record_id),
                        &task.original_name,
                        TaskRunItemStatus::Missing.as_str(),
                        Some(&e.to_string()),
                        Some(task.size),
                    );
                    continue;
                }
            };
            match ObjectPublisher::new(client)
                .publish_object(vault, device, path, &task.hash)
                .await
            {
                Ok(result) => {
                    if matches!(result, PublishResult::Published { .. }) {
                        report.uploaded += 1;
                    }
                    report.verified += 1;
                    db.update_task_status(&task.task_id, "backed_up", None)?;
                }
                Err(e) => {
                    report.failed += 1;
                    db.update_task_status(&task.task_id, "retryable_failed", Some(&e.to_string()))?;
                    record_archive_item(
                        db,
                        run_id,
                        &task,
                        TaskRunItemStatus::Failed,
                        Some(&e.to_string()),
                    )?;
                    progress.on_item(
                        "archive",
                        Some(&task.task_id),
                        Some(&task.record_id),
                        &task.original_name,
                        TaskRunItemStatus::Failed.as_str(),
                        Some(&e.to_string()),
                        Some(task.size),
                    );
                }
            }
        }
    }
    progress.on_stage(
        "archive",
        "success",
        Some(&format!(
            "上传 {} · 校验 {} · 失败 {}",
            report.uploaded, report.verified, report.failed
        )),
    );
    Ok(report)
}

/// 将归档失败/缺失写入运行明细。
fn record_archive_item(
    db: &mut Database,
    run_id: Option<&str>,
    task: &chatvault_index::upload_queue::PendingUpload,
    status: TaskRunItemStatus,
    error: Option<&str>,
) -> Result<()> {
    let Some(run_id) = run_id else {
        return Ok(());
    };
    db.add_task_run_item(
        run_id,
        &NewTaskRunItem {
            stage: "archive",
            record_id: Some(&task.record_id),
            object_id: None,
            task_id: Some(&task.task_id),
            name: &task.original_name,
            status: status.as_str(),
            error_code: None,
            error_message: error,
            size: Some(task.size as i64),
        },
    )?;
    Ok(())
}
