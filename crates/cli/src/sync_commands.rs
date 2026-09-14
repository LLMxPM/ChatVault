// ChatVault CLI 定时运行与元数据同步入口：使用持久化身份和共享核心。
use super::*;
use chatvault_core::models::{
    CollectSource, TaskRunKind, TaskRunStageName, TaskRunStageStatus, TaskRunStatus,
    GENERIC_FOLDER_SOURCE_TYPE, WECHAT_WINDOWS_4_SOURCE_TYPE,
};
use chatvault_sync::archive_pending_with_progress;
use chatvault_sync::NoopProgressSink;
use std::path::Path;

/// 定时任务：读取本地设置，增量扫描采集目录并归档到 WebDAV，并写入运行日志
pub(super) async fn handle_scheduled_run(db_path: &PathBuf) -> Result<()> {
    println!("=== ChatVault 定时增量扫描与归档 ===");
    let mut db = Database::open(db_path)?;
    // 定时进程独立启动：仅在此清理一次，避免与桌面端并发时误标进行中运行。
    db.maintain_task_runs_on_startup()?;

    let device_id = db.ensure_device_identity()?;
    let vault_id = db
        .get_setting("vault_id")?
        .unwrap_or_else(|| "chatvault-default".to_string());
    let webdav_url = db.get_setting("webdav_url")?.unwrap_or_default();
    let webdav_user = db.get_setting("webdav_username")?.unwrap_or_default();
    let collect_sources_raw = db
        .get_setting("collect_sources")?
        .unwrap_or_else(|| "[]".to_string());
    let collect_sources: Vec<CollectSource> =
        serde_json::from_str(&collect_sources_raw).context("解析采集源配置失败")?;
    // 与桌面「立即运行」共用账号勾选：None=从未配置（全选）；空数组=全部取消。
    let selected_accounts = load_selected_accounts(&db)?;
    let scan_started_ms = Utc::now().timestamp_millis();

    let webdav_configured = !webdav_url.trim().is_empty();
    let run_id = db.start_task_run(TaskRunKind::Pipeline, "schedule", webdav_configured, "cli")?;
    db.start_task_run_stage(&run_id, TaskRunStageName::Scan)?;

    let mut indexed = 0usize;
    let mut skipped = 0usize;
    let mut discovered = 0usize;

    // 1. 按配置的适配器扫描采集源（有检查点则增量）。
    for source in &collect_sources {
        if db.task_run_cancel_requested(&run_id)? {
            return finish_cancelled_run(
                &mut db,
                &run_id,
                TaskRunStageName::Scan,
                discovered,
                indexed,
                skipped,
            );
        }
        match source.source_type.as_str() {
            WECHAT_WINDOWS_4_SOURCE_TYPE => scan_wechat_source(
                &mut db,
                &device_id,
                &source.path,
                selected_accounts.as_deref(),
                scan_started_ms,
                source.enable_videos,
                &mut indexed,
                &mut skipped,
                &mut discovered,
            )?,
            GENERIC_FOLDER_SOURCE_TYPE => scan_generic_source(
                &mut db,
                &device_id,
                &source.path,
                scan_started_ms,
                &mut indexed,
                &mut skipped,
                &mut discovered,
            )?,
            other => println!("[-] 跳过未知采集源类型 {}: {}", other, source.path),
        }
        let _ = db.touch_task_run_progress(
            &run_id,
            TaskRunStageName::Scan,
            indexed,
            discovered.max(indexed),
            Some(&source.path),
        );
    }

    println!(
        "[*] 候选 {} 个，跳过未变 {}，新入库 {}",
        discovered, skipped, indexed
    );

    db.finish_task_run_stage(
        &run_id,
        TaskRunStageName::Scan,
        TaskRunStageStatus::Success,
        Some(
            &serde_json::json!({
                "discovered": discovered,
                "newObjects": indexed,
                "skipped": skipped,
            })
            .to_string(),
        ),
        Some(&format!(
            "候选 {discovered} · 新增 {indexed} · 跳过 {skipped}"
        )),
    )?;

    // 3. 若未配置 WebDAV 则仅完成本地扫描
    if !webdav_configured {
        println!("[-] 未配置 WebDAV，跳过归档");
        db.finish_task_run_stage(
            &run_id,
            TaskRunStageName::Archive,
            TaskRunStageStatus::Skipped,
            None,
            Some("未配置 WebDAV"),
        )?;
        db.finish_task_run(
            &run_id,
            TaskRunStatus::Success,
            Some(
                &serde_json::json!({
                    "discovered": discovered,
                    "newObjects": indexed,
                    "skipped": skipped,
                })
                .to_string(),
            ),
            None,
        )?;
        return Ok(());
    }

    // 4. 从系统凭据管理器读取密码
    let password = {
        let key = format!(
            "{}:{}",
            webdav_url.trim().trim_end_matches('/'),
            webdav_user.trim()
        );
        keyring::Entry::new("chatvault-webdav", &key)
            .and_then(|e| e.get_password())
            .ok()
    };

    let cfg = WebDavConfig {
        base_url: webdav_url.clone(),
        username: Some(webdav_user.clone()),
        password: password.clone(),
    };
    let client = WebDavClient::new(cfg).context("创建 WebDAV 客户端失败")?;

    db.start_task_run_stage(&run_id, TaskRunStageName::Archive)?;
    let archive = match archive_pending_with_progress(
        &client,
        &mut db,
        &vault_id,
        &device_id,
        0,
        &mut NoopProgressSink,
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
            return Err(e.into());
        }
    };
    if archive.cancelled {
        return finish_cancelled_after_archive(
            &mut db, &run_id, discovered, indexed, skipped, &archive,
        );
    }
    println!(
        "归档完成：新上传 {}，已校验 {}，失败 {}",
        archive.uploaded, archive.verified, archive.failed
    );
    db.finish_task_run_stage(
        &run_id,
        TaskRunStageName::Archive,
        TaskRunStageStatus::Success,
        Some(
            &serde_json::json!({
                "uploaded": archive.uploaded,
                "verified": archive.verified,
                "failed": archive.failed,
            })
            .to_string(),
        ),
        Some(&format!(
            "上传 {} · 失败 {}",
            archive.uploaded, archive.failed
        )),
    )?;

    // 归档后发布本机元数据日志，并拉取其他设备
    db.start_task_run_stage(&run_id, TaskRunStageName::Publish)?;
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
                let _ = finish_partial_scheduled_run(
                    &mut db, &run_id, discovered, indexed, skipped, &archive, &msg,
                );
                return Err(e.into());
            }
        };
    db.finish_task_run_stage(
        &run_id,
        TaskRunStageName::Publish,
        TaskRunStageStatus::Success,
        Some(&serde_json::json!({ "publishedSeq": published }).to_string()),
        Some(&format!("已发布 seq={published}")),
    )?;

    db.start_task_run_stage(&run_id, TaskRunStageName::Pull)?;
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
                let _ = finish_partial_scheduled_run(
                    &mut db, &run_id, discovered, indexed, skipped, &archive, &msg,
                );
                return Err(e.into());
            }
        };
    db.finish_task_run_stage(
        &run_id,
        TaskRunStageName::Pull,
        TaskRunStageStatus::Success,
        Some(&serde_json::json!({ "applied": applied }).to_string()),
        Some(&format!("应用远端事件 {applied} 条")),
    )?;

    let item_counts = db.count_task_run_items_by_status(&run_id)?;
    let failed_items: i64 = item_counts
        .iter()
        .filter(|(s, _)| s != "skipped")
        .map(|(_, c)| *c)
        .sum();
    let status = if archive.failed > 0 || failed_items > 0 {
        TaskRunStatus::Partial
    } else {
        TaskRunStatus::Success
    };
    db.finish_task_run(
        &run_id,
        status,
        Some(
            &serde_json::json!({
                "discovered": discovered,
                "newObjects": indexed,
                "skipped": skipped,
                "uploaded": archive.uploaded,
                "failed": archive.failed,
                "publishedSeq": published,
                "applied": applied,
                "failedItems": failed_items,
            })
            .to_string(),
        ),
        None,
    )?;

    if archive.failed > 0 {
        anyhow::bail!("存在归档失败任务，已保存失败原因并等待重试");
    }
    Ok(())
}

/// 扫描阶段观察到取消：当前阶段 skipped，整 run cancelled。
fn finish_cancelled_run(
    db: &mut Database,
    run_id: &str,
    stage: TaskRunStageName,
    discovered: usize,
    indexed: usize,
    skipped: usize,
) -> Result<()> {
    db.finish_task_run_stage(
        run_id,
        stage,
        TaskRunStageStatus::Skipped,
        None,
        Some("用户取消"),
    )?;
    db.finish_task_run(
        run_id,
        TaskRunStatus::Cancelled,
        Some(
            &serde_json::json!({
                "discovered": discovered,
                "newObjects": indexed,
                "skipped": skipped,
                "cancelled": true,
            })
            .to_string(),
        ),
        Some("用户取消"),
    )?;
    println!("[*] 运行已取消");
    Ok(())
}

/// 归档阶段取消：保留扫描结果，archive 标 skipped。
fn finish_cancelled_after_archive(
    db: &mut Database,
    run_id: &str,
    discovered: usize,
    indexed: usize,
    skipped: usize,
    archive: &chatvault_sync::ArchiveReport,
) -> Result<()> {
    db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Archive,
        TaskRunStageStatus::Skipped,
        Some(
            &serde_json::json!({
                "uploaded": archive.uploaded,
                "verified": archive.verified,
                "failed": archive.failed,
                "cancelled": true,
            })
            .to_string(),
        ),
        Some("用户取消"),
    )?;
    db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Publish,
        TaskRunStageStatus::Skipped,
        None,
        Some("用户取消"),
    )?;
    db.finish_task_run_stage(
        run_id,
        TaskRunStageName::Pull,
        TaskRunStageStatus::Skipped,
        None,
        Some("用户取消"),
    )?;
    db.finish_task_run(
        run_id,
        TaskRunStatus::Cancelled,
        Some(
            &serde_json::json!({
                "discovered": discovered,
                "newObjects": indexed,
                "skipped": skipped,
                "uploaded": archive.uploaded,
                "failed": archive.failed,
                "cancelled": true,
            })
            .to_string(),
        ),
        Some("用户取消"),
    )?;
    println!("[*] 运行已取消");
    Ok(())
}

/// 发布/拉取失败时仍把已有扫描与归档结果记为 partial。
#[allow(clippy::too_many_arguments)]
fn finish_partial_scheduled_run(
    db: &mut Database,
    run_id: &str,
    discovered: usize,
    indexed: usize,
    skipped: usize,
    archive: &chatvault_sync::ArchiveReport,
    error: &str,
) -> Result<()> {
    db.finish_task_run(
        run_id,
        TaskRunStatus::Partial,
        Some(
            &serde_json::json!({
                "discovered": discovered,
                "newObjects": indexed,
                "skipped": skipped,
                "uploaded": archive.uploaded,
                "failed": archive.failed,
                "error": error,
            })
            .to_string(),
        ),
        Some(error),
    )?;
    Ok(())
}

/// 桌面持久化的微信账号勾选项；字段与前端 collect_selected_accounts 一致。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectedAccount {
    source_root: String,
    source_account_id: String,
}

/// 读取账号勾选：设置缺失或 null 表示从未配置（全选）。
fn load_selected_accounts(db: &Database) -> Result<Option<Vec<SelectedAccount>>> {
    let Some(raw) = db.get_setting("collect_selected_accounts")? else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(None);
    }
    let accounts: Vec<SelectedAccount> =
        serde_json::from_str(trimmed).context("解析账号勾选配置失败")?;
    Ok(Some(accounts))
}

/// 判断微信账号是否在勾选范围内；selected 为 None 时全选。
fn is_selected_account(
    selected: Option<&[SelectedAccount]>,
    root: &Path,
    account_id: &str,
) -> bool {
    let Some(targets) = selected else {
        return true;
    };
    let root_key = chatvault_core::normalize_scan_key(&root.to_string_lossy());
    targets.iter().any(|target| {
        target.source_account_id == account_id
            && chatvault_core::normalize_scan_key(&target.source_root) == root_key
    })
}

/// 按微信 4.x 适配器扫描一个配置的根目录。
#[allow(clippy::too_many_arguments)]
fn scan_wechat_source(
    db: &mut Database,
    device_id: &str,
    root_path: &str,
    selected_accounts: Option<&[SelectedAccount]>,
    scan_started_ms: i64,
    enable_videos: bool,
    indexed: &mut usize,
    skipped: &mut usize,
    discovered: &mut usize,
) -> Result<()> {
    let root = PathBuf::from(root_path);
    if !root.is_dir() {
        println!("[-] 微信 4.x 目录不存在，跳过: {}", root_path);
        return Ok(());
    }
    let accounts = WeChat4Detector::find_accounts(&root)?;
    for acc in accounts {
        if !is_selected_account(selected_accounts, &root, &acc.source_account_id) {
            println!(
                "[*] 跳过未勾选微信账号 [{}]",
                acc.source_account_id
            );
            continue;
        }
        println!("[*] 扫描微信账号 [{}]", acc.source_account_id);

        // 媒体根 1: msg/file
        scan_one_media_root(
            db,
            device_id,
            &acc.files_dir,
            &acc.source_account_id,
            scan_started_ms,
            indexed,
            skipped,
            discovered,
            true,
        )?;

        // 媒体根 2: msg/video
        if !enable_videos {
            println!("    [video] 已按采集源配置关闭视频识别");
            continue;
        }
        scan_one_media_root(
            db,
            device_id,
            &acc.video_dir,
            &acc.source_account_id,
            scan_started_ms,
            indexed,
            skipped,
            discovered,
            false,
        )?;
    }
    Ok(())
}

/// 扫描单个媒体根
#[allow(clippy::too_many_arguments)]
fn scan_one_media_root(
    db: &mut Database,
    device_id: &str,
    media_root: &std::path::Path,
    account_id: &str,
    scan_started_ms: i64,
    indexed: &mut usize,
    skipped: &mut usize,
    discovered: &mut usize,
    use_file_parser: bool,
) -> Result<()> {
    let root_s = media_root.to_string_lossy().to_string();
    let since = resolve_since(db, &root_s)?;
    let walked = if use_file_parser {
        if !media_root.exists() {
            return Ok(());
        }
        WeChat4Parser::parse_folder_since(media_root, Some(account_id), since)?
    } else {
        let fake = adapter_wechat_windows::WeChatAccount {
            source_account_id: account_id.to_string(),
            root_dir: media_root
                .parent()
                .and_then(|p| p.parent())
                .unwrap_or(media_root)
                .to_path_buf(),
            files_dir: media_root.to_path_buf(),
            video_dir: media_root.to_path_buf(),
        };
        WeChat4Parser::parse_account_videos_since(&fake, since)?
    };
    let changed_known = if since.is_some() {
        db.list_changed_known_files(&root_s)?
    } else {
        Vec::new()
    };
    let files = merge_changed_known(
        walked,
        changed_known,
        WECHAT_WINDOWS_4_SOURCE_TYPE,
        Some(account_id),
        None,
        if use_file_parser {
            Some(media_root)
        } else {
            None
        },
    );
    *discovered += files.len();
    let complete = process_files(db, device_id, &files, indexed, skipped)?;
    if complete {
        db.mark_scan_started(
            &root_s,
            WECHAT_WINDOWS_4_SOURCE_TYPE,
            Some(account_id),
            scan_started_ms,
        )?;
    } else {
        println!("[-] 媒体根 {} 存在未完成候选，保留原扫描检查点", root_s);
    }
    Ok(())
}

/// 扫描一个通用附件目录采集源。
#[allow(clippy::too_many_arguments)]
fn scan_generic_source(
    db: &mut Database,
    device_id: &str,
    root_path: &str,
    scan_started_ms: i64,
    indexed: &mut usize,
    skipped: &mut usize,
    discovered: &mut usize,
) -> Result<()> {
    let root = PathBuf::from(root_path);
    if !root.is_dir() {
        println!("[-] 附件目录不存在，跳过: {}", root_path);
        return Ok(());
    }
    println!("[*] 扫描附件目录: {}", root_path);
    let since = resolve_since(db, root_path)?;
    let walked = GenericFolderParser::parse_with_since(&root, since)?;
    let changed_known = if since.is_some() {
        db.list_changed_known_files(root_path)?
    } else {
        Vec::new()
    };
    let files = merge_changed_known(
        walked,
        changed_known,
        GENERIC_FOLDER_SOURCE_TYPE,
        None,
        None,
        None,
    );
    *discovered += files.len();
    let complete = process_files(db, device_id, &files, indexed, skipped)?;
    if complete {
        db.mark_scan_started(root_path, GENERIC_FOLDER_SOURCE_TYPE, None, scan_started_ms)?;
    } else {
        println!("[-] 附件目录存在未完成候选，保留原扫描检查点");
    }
    Ok(())
}

/// 定时任务始终使用增量：无检查点时自动退化为全量
fn resolve_since(db: &Database, root: &str) -> Result<Option<std::time::SystemTime>> {
    let ms = db.get_scan_started_ms(root)?;
    Ok(ms.map(chatvault_index::system_time_from_ms))
}

/// 合并目录发现与已知文件内容变更
fn merge_changed_known(
    walked: Vec<DiscoveredFile>,
    changed_known: Vec<chatvault_index::KnownLocalFile>,
    source_type: &str,
    source_account_id: Option<&str>,
    source_conversation_id: Option<String>,
    source_root: Option<&std::path::Path>,
) -> Vec<DiscoveredFile> {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged = Vec::with_capacity(walked.len() + changed_known.len());
    for file in walked {
        let key = chatvault_core::normalize_scan_key(&file.absolute_path);
        if seen.insert(key) {
            merged.push(file);
        }
    }
    for known in changed_known {
        let key = chatvault_core::normalize_scan_key(&known.original_path);
        if seen.contains(&key) {
            continue;
        }
        let conversation_id = source_root
            .and_then(|root| WeChat4Parser::conversation_id_for_path(root, &known.original_path))
            .or_else(|| source_conversation_id.clone());
        if let Some(file) = known.to_discovered(source_type, source_account_id, conversation_id) {
            seen.insert(key);
            merged.push(file);
        }
    }
    merged
}

/// 仅对需要处理的文件做稳定性检测并入库，返回是否全部完成。
///
/// 未稳定或入库失败的文件会使本轮检查点保持不变，等待下次扫描重试。
fn process_files(
    db: &mut Database,
    device_id: &str,
    files: &[DiscoveredFile],
    indexed: &mut usize,
    skipped: &mut usize,
) -> Result<bool> {
    let mut complete = true;
    for file in files {
        if db.path_is_current(&file.absolute_path)? {
            *skipped += 1;
            continue;
        }
        let stable = check_file_stability_sync(&file.absolute_path, Duration::from_millis(50))
            .unwrap_or(false);
        if !stable {
            complete = false;
            continue;
        }
        match db.ingest_file(file, device_id) {
            Ok(IngestResult::Indexed { .. }) => *indexed += 1,
            Ok(IngestResult::Skipped { .. }) => *skipped += 1,
            Err(e) => {
                complete = false;
                eprintln!("[-] 入库异常 {}: {}", file.file_name, e);
            }
        }
    }
    Ok(complete)
}

/// 发布本机元数据日志
pub(super) async fn handle_sync_publish(
    url: &str,
    user: Option<String>,
    pass: Option<String>,
    vault_id: &str,
    db_path: &PathBuf,
    device_id: &str,
) -> Result<()> {
    println!("=== 发布本机元数据日志 ===");
    let mut db = Database::open(db_path)?;
    let device_id = resolve_device(&mut db, device_id)?;
    let config = WebDavConfig {
        base_url: url.to_string(),
        username: user,
        password: pass,
    };
    let client = WebDavClient::new(config)?;
    let seq =
        chatvault_sync::publish_pending_events(&client, &mut db, vault_id, &device_id).await?;
    println!("[+] 已推进本机游标至 seq={}", seq);
    Ok(())
}

/// 拉取远端日志并合并
pub(super) async fn handle_sync_pull(
    url: &str,
    user: Option<String>,
    pass: Option<String>,
    vault_id: &str,
    db_path: &PathBuf,
    device_id: &str,
) -> Result<()> {
    println!("=== 拉取远端元数据并合并 ===");
    let mut db = Database::open(db_path)?;
    let device_id = resolve_device(&mut db, device_id)?;
    let config = WebDavConfig {
        base_url: url.to_string(),
        username: user,
        password: pass,
    };
    let client = WebDavClient::new(config)?;
    let applied = chatvault_sync::pull_and_apply(&client, &mut db, vault_id, &device_id).await?;
    println!("[+] 本次应用 {} 条新事件", applied);
    let stats = db.get_stats()?;
    println!(
        "    本地记录数: {}, 对象数: {}",
        stats.total_records, stats.total_objects
    );
    Ok(())
}

/// 空索引恢复
pub(super) async fn handle_restore(
    url: &str,
    user: Option<String>,
    pass: Option<String>,
    vault_id: &str,
    db_path: &PathBuf,
    device_id: &str,
) -> Result<()> {
    println!("=== 从 WebDAV 恢复本地索引 ===");
    let mut db = Database::open(db_path)?;
    let device_id = resolve_device(&mut db, device_id)?;
    let config = WebDavConfig {
        base_url: url.to_string(),
        username: user,
        password: pass,
    };
    let client = WebDavClient::new(config)?;
    let report =
        chatvault_sync::restore_from_remote(&client, &mut db, vault_id, &device_id).await?;
    println!(
        "[+] 恢复完成: 应用事件 {}, 来源记录 {}, 内容对象 {}",
        report.applied_events, report.total_records, report.total_objects
    );
    Ok(())
}
