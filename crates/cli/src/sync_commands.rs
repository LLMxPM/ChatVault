// ChatVault CLI 定时运行与元数据同步入口：使用持久化身份和共享扫描编排。
use super::*;
use chatvault_core::models::{
    CollectSource, TaskRunKind, TaskRunStageName, TaskRunStageStatus, TaskRunStatus,
    GENERIC_FOLDER_SOURCE_TYPE, WECHAT_WINDOWS_4_SOURCE_TYPE, WXWORK_WINDOWS_SOURCE_TYPE,
};
use chatvault_scan::{
    scan_generic_source, scan_wechat_source, scan_wxwork_source, AccountTarget, ScanEvent,
    ScanReport, ScanRequest,
};
use chatvault_sync::archive_pending_with_progress;
use chatvault_sync::NoopProgressSink;
use chrono::Utc;

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

    let on_event = |event: ScanEvent| print_scan_event(&event);
    let mut report = ScanReport::default();

    // 1. 按配置的适配器扫描采集源（有检查点则增量），来源边界检查取消。
    for source in &collect_sources {
        if db.task_run_cancel_requested(&run_id)? {
            return finish_cancelled_run(
                &mut db,
                &run_id,
                TaskRunStageName::Scan,
                report.discovered,
                report.indexed,
                report.skipped,
            );
        }
        let req = ScanRequest {
            device_id: &device_id,
            full_scan: false,
            target_accounts: selected_accounts.as_deref(),
            scan_started_ms,
            should_cancel: None,
            on_event: Some(&on_event),
            on_source_done: None,
        };
        let root = PathBuf::from(&source.path);
        let partial = match source.source_type.as_str() {
            WECHAT_WINDOWS_4_SOURCE_TYPE => {
                scan_wechat_source(&mut db, &root, source.enable_videos, &req)?
            }
            WXWORK_WINDOWS_SOURCE_TYPE => {
                scan_wxwork_source(&mut db, &root, source.enable_videos, &req)?
            }
            GENERIC_FOLDER_SOURCE_TYPE => scan_generic_source(&mut db, &root, &req)?,
            other => {
                println!("[-] 跳过未知采集源类型 {}: {}", other, source.path);
                continue;
            }
        };
        report.merge(partial);
        println!(
            "[*] 采集源完成 {}: 累计候选 {} · 新入库 {} · 跳过 {}",
            source.path, report.discovered, report.indexed, report.skipped
        );
        let _ = db.touch_task_run_progress(
            &run_id,
            TaskRunStageName::Scan,
            report.indexed,
            report.discovered.max(report.indexed),
            Some(&source.path),
        );
    }

    let discovered = report.discovered;
    let indexed = report.indexed;
    let skipped = report.skipped;

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
                "newObjects": report.new_objects,
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
                    "newObjects": report.new_objects,
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
                "newObjects": report.new_objects,
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

/// 打印共享扫描事件。
fn print_scan_event(event: &ScanEvent) {
    match event {
        ScanEvent::SourceStart { source_type, path } => {
            println!("[*] 扫描采集源 {source_type}: {path}");
        }
        ScanEvent::UnknownSourceType { source_type, path } => {
            println!("[-] 跳过未知采集源类型 {source_type}: {path}");
        }
        ScanEvent::SourceMissing { path, label } => {
            println!("[-] {label}: {path}");
        }
        ScanEvent::AccountListed { accounts } => {
            println!("[+] 发现账号 {} 个", accounts.len());
        }
        ScanEvent::AccountSkipped { account_id } => {
            println!("[*] 跳过未勾选账号 [{account_id}]");
        }
        ScanEvent::AccountStart { account_id } => {
            println!("[*] 扫描账号 [{account_id}]");
        }
        ScanEvent::MediaRootStart { kind, path } => {
            println!("    [{}] {}", kind.as_str(), path);
        }
        ScanEvent::MediaRootCandidates { kind, path, count } => {
            println!("    [{}] 候选 {} 个文件 ({path})", kind.as_str(), count);
        }
        ScanEvent::MediaRootIncomplete { path } => {
            println!("[-] 媒体根 {path} 存在未完成候选，保留原扫描检查点");
        }
        ScanEvent::VideosDisabled { account_id } => {
            println!("    [video] 账号 {account_id} 已按采集源配置关闭视频识别");
        }
    }
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

/// 桌面持久化的账号勾选项；字段与前端 collect_selected_accounts 一致。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectedAccount {
    source_root: String,
    source_account_id: String,
}

/// 读取账号勾选：设置缺失或 null 表示从未配置（全选）。
fn load_selected_accounts(db: &Database) -> Result<Option<Vec<AccountTarget>>> {
    let Some(raw) = db.get_setting("collect_selected_accounts")? else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(None);
    }
    let accounts: Vec<SelectedAccount> =
        serde_json::from_str(trimmed).context("解析账号勾选配置失败")?;
    Ok(Some(
        accounts
            .into_iter()
            .map(|a| AccountTarget {
                source_root: a.source_root,
                source_account_id: a.source_account_id,
            })
            .collect(),
    ))
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

// 暴露给 handle_scan 使用的共享扫描入口
pub(super) fn scan_path_with_shared(
    db: &mut Database,
    device_id: &str,
    target: &str,
    full: bool,
) -> Result<ScanReport> {
    let on_event = |event: ScanEvent| print_scan_event(&event);
    let scan_started_ms = Utc::now().timestamp_millis();
    let wechat_root = if target.eq_ignore_ascii_case("wechat") {
        Some(chatvault_scan::detect_wechat_root().context("探测微信 4.x 根目录失败")?)
    } else if target.eq_ignore_ascii_case("wxwork") {
        // 企业微信：作为通用路径扫描账号型根
        let root = chatvault_scan::detect_wxwork_root().context("探测企业微信根目录失败")?;
        let enable_videos =
            load_source_video_setting(db, &root.to_string_lossy(), WXWORK_WINDOWS_SOURCE_TYPE);
        let req = ScanRequest {
            device_id,
            full_scan: full,
            target_accounts: None,
            scan_started_ms,
            should_cancel: None,
            on_event: Some(&on_event),
            on_source_done: None,
        };
        return Ok(scan_wxwork_source(db, &root, enable_videos, &req)?);
    } else {
        adapter_wechat_windows::WeChat4Detector::validate_root(target).ok()
    };

    if let Some(root) = wechat_root {
        let enable_videos =
            load_source_video_setting(db, &root.to_string_lossy(), WECHAT_WINDOWS_4_SOURCE_TYPE);
        let req = ScanRequest {
            device_id,
            full_scan: full,
            target_accounts: None,
            scan_started_ms,
            should_cancel: None,
            on_event: Some(&on_event),
            on_source_done: None,
        };
        return Ok(scan_wechat_source(db, &root, enable_videos, &req)?);
    }

    // 可能是企业微信手动路径
    if adapter_wxwork_windows::WxWorkDetector::validate_root(target).is_ok() {
        let enable_videos = load_source_video_setting(db, target, WXWORK_WINDOWS_SOURCE_TYPE);
        let root = PathBuf::from(target);
        let req = ScanRequest {
            device_id,
            full_scan: full,
            target_accounts: None,
            scan_started_ms,
            should_cancel: None,
            on_event: Some(&on_event),
            on_source_done: None,
        };
        return Ok(scan_wxwork_source(db, &root, enable_videos, &req)?);
    }

    println!("[*] 正在扫描通用目录: {target}");
    let root = PathBuf::from(target);
    let req = ScanRequest {
        device_id,
        full_scan: full,
        target_accounts: None,
        scan_started_ms,
        should_cancel: None,
        on_event: Some(&on_event),
        on_source_done: None,
    };
    Ok(scan_generic_source(db, &root, &req)?)
}

/// 从持久化采集源读取指定类型的视频识别开关；未配置时默认开启。
fn load_source_video_setting(db: &Database, root: &str, source_type: &str) -> bool {
    let raw = db
        .get_setting("collect_sources")
        .ok()
        .flatten()
        .unwrap_or_else(|| "[]".to_string());
    let sources: Vec<CollectSource> = serde_json::from_str(&raw).unwrap_or_default();
    let key = chatvault_core::normalize_scan_key(root);
    sources
        .into_iter()
        .find(|source| {
            source.source_type == source_type
                && chatvault_core::normalize_scan_key(&source.path) == key
        })
        .map(|source| source.enable_videos)
        .unwrap_or(true)
}
