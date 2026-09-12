// ChatVault CLI 定时运行与元数据同步入口：使用持久化身份和共享核心。
use super::*;

/// 定时任务：读取本地设置，增量扫描采集目录并归档到 WebDAV
pub(super) async fn handle_scheduled_run(db_path: &PathBuf) -> Result<()> {
    println!("=== ChatVault 定时增量扫描与归档 ===");
    let mut db = Database::open(db_path)?;

    let device_id = db.ensure_device_identity()?;
    let vault_id = db
        .get_setting("vault_id")?
        .unwrap_or_else(|| "default-vault".to_string());
    let webdav_url = db.get_setting("webdav_url")?.unwrap_or_default();
    let webdav_user = db.get_setting("webdav_username")?.unwrap_or_default();
    let collect_dirs_raw = db
        .get_setting("collect_dirs")?
        .unwrap_or_else(|| "[]".to_string());
    let collect_dirs: Vec<String> = serde_json::from_str(&collect_dirs_raw).unwrap_or_default();
    let scan_started_ms = Utc::now().timestamp_millis();

    let mut indexed = 0usize;
    let mut skipped = 0usize;
    let mut discovered = 0usize;

    // 1. 扫描微信（有检查点则增量）
    if let Ok(root) = WeChat4Detector::detect_root() {
        let accounts = WeChat4Detector::find_accounts(&root)?;
        for acc in accounts {
            println!("[*] 扫描微信账号 [{}]", acc.account_id);
            let files_root = acc.files_dir.to_string_lossy().to_string();
            let since = resolve_since(&db, &files_root)?;
            let walked = WeChat4Parser::parse_account_files_since(&acc, since)?;
            let changed_known = if since.is_some() {
                db.list_changed_known_files(&files_root)?
            } else {
                Vec::new()
            };
            let files = merge_changed_known(
                walked,
                changed_known,
                "wechat-windows-4",
                Some(&acc.account_id),
                None,
            );
            discovered += files.len();
            let complete = process_files(&mut db, &device_id, &files, &mut indexed, &mut skipped)?;
            if complete {
                db.mark_scan_started(
                    &files_root,
                    "wechat-windows-4",
                    Some(&acc.account_id),
                    scan_started_ms,
                )?;
            } else {
                println!(
                    "[-] 微信账号 {} 存在未完成候选，保留原扫描检查点",
                    acc.account_id
                );
            }
        }
    }

    // 2. 扫描设置中的采集目录（有检查点则增量）
    for dir in &collect_dirs {
        let p = PathBuf::from(dir);
        if !p.exists() {
            continue;
        }
        println!("[*] 扫描采集目录: {}", dir);
        let since = resolve_since(&db, dir)?;
        let walked = GenericFolderParser::parse_with_since(&p, since)?;
        let changed_known = if since.is_some() {
            db.list_changed_known_files(dir)?
        } else {
            Vec::new()
        };
        let files = merge_changed_known(walked, changed_known, "generic-folder", None, None);
        discovered += files.len();
        let complete = process_files(&mut db, &device_id, &files, &mut indexed, &mut skipped)?;
        if complete {
            db.mark_scan_started(dir, "generic-folder", None, scan_started_ms)?;
        } else {
            println!("[-] 通用目录存在未完成候选，保留原扫描检查点");
        }
    }

    println!(
        "[*] 候选 {} 个，跳过未变 {}，新入库 {}",
        discovered, skipped, indexed
    );

    // 3. 若未配置 WebDAV 则仅完成本地扫描
    if webdav_url.is_empty() {
        println!("[-] 未配置 WebDAV，跳过归档");
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

    handle_archive(
        &webdav_url,
        Some(webdav_user.clone()),
        password.clone(),
        &vault_id,
        db_path,
        0,
    )
    .await?;

    // 归档后发布本机元数据日志，并拉取其他设备
    if !webdav_url.is_empty() {
        let cfg = WebDavConfig {
            base_url: webdav_url.clone(),
            username: Some(webdav_user.clone()),
            password: password.clone(),
        };
        if let Ok(client) = WebDavClient::new(cfg) {
            chatvault_sync::publish_pending_events(&client, &mut db, &vault_id, &device_id).await?;
            chatvault_sync::pull_and_apply(&client, &mut db, &vault_id, &device_id).await?;
        }
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
    account_id: Option<&str>,
    conversation_hint: Option<String>,
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
        if let Some(file) = known.to_discovered(source_type, account_id, conversation_hint.clone())
        {
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
