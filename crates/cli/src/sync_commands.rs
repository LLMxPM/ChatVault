// ChatVault CLI 定时运行与元数据同步入口：使用持久化身份和共享核心。
use super::*;

/// 定时任务：读取本地设置，扫描采集目录并归档到 WebDAV
pub(super) async fn handle_scheduled_run(db_path: &PathBuf) -> Result<()> {
    println!("=== ChatVault 定时扫描与归档 ===");
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

    // 1. 扫描微信
    let mut files: Vec<DiscoveredFile> = Vec::new();
    if let Ok(root) = WeChat4Detector::detect_root() {
        let accounts = WeChat4Detector::find_accounts(&root)?;
        for acc in accounts {
            println!("[*] 扫描微信账号 [{}]", acc.account_id);
            if let Ok(acc_files) = WeChat4Parser::parse_account_files(&acc) {
                files.extend(acc_files);
            }
        }
    }

    // 2. 扫描设置中的采集目录
    for dir in &collect_dirs {
        let p = PathBuf::from(dir);
        if p.exists() {
            println!("[*] 扫描采集目录: {}", dir);
            if let Ok(f) = GenericFolderParser::parse(&p) {
                files.extend(f);
            }
        }
    }

    println!("[*] 候选文件 {} 个，开始入库...", files.len());
    let mut indexed = 0usize;
    for file in &files {
        let stable = check_file_stability_sync(&file.absolute_path, Duration::from_millis(50))
            .unwrap_or(false);
        if !stable {
            continue;
        }
        if matches!(
            db.ingest_file(file, &device_id),
            Ok(IngestResult::Indexed { .. })
        ) {
            indexed += 1;
        }
    }
    println!("[+] 本次新入库 {} 条", indexed);

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
