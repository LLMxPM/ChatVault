// ChatVault 共享归档队列：桌面与计划任务采用相同重试、暂存和校验规则。
use chatvault_core::{error::Result, models::VaultConfig};
use chatvault_index::Database;
use chatvault_webdav::{ObjectPublisher, PublishResult, WebDavClient};

/// 一轮归档结果；失败任务保留错误并按退避策略等待重试。
#[derive(Default)]
pub struct ArchiveReport {
    pub uploaded: usize,
    pub verified: usize,
    pub failed: usize,
}

/// 归档当前可执行任务；limit 为 0 时持续取批次直到当前队列处理完。
pub async fn archive_pending(
    client: &WebDavClient,
    db: &mut Database,
    vault: &str,
    device: &str,
    limit: usize,
) -> Result<ArchiveReport> {
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
            let path = match db.upload_source(&task.task_id) {
                Ok(path) => path,
                Err(e) => {
                    report.failed += 1;
                    db.update_task_status(&task.task_id, "missing", Some(&e.to_string()))?;
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
                }
            }
        }
    }
    Ok(report)
}
