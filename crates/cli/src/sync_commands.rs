// ChatVault CLI 定时运行与元数据同步入口：使用持久化身份和共享核心。
use super::*;
use chatvault_core::models::{
    CollectSource, GENERIC_FOLDER_SOURCE_TYPE, WECHAT_WINDOWS_4_SOURCE_TYPE,
};

/// 定时任务：读取本地设置，增量扫描采集目录并归档到 WebDAV
pub(super) async fn handle_scheduled_run(db_path: &PathBuf) -> Result<()> {
    println!("=== ChatVault 定时增量扫描与归档 ===");
    let mut db = Database::open(db_path)?;

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
    let scan_started_ms = Utc::now().timestamp_millis();

    let mut indexed = 0usize;
    let mut skipped = 0usize;
    let mut discovered = 0usize;

    // 1. 按配置的适配器扫描采集源（有检查点则增量）。
    for source in &collect_sources {
        match source.source_type.as_str() {
            WECHAT_WINDOWS_4_SOURCE_TYPE => scan_wechat_source(
                &mut db,
                &device_id,
                &source.path,
                scan_started_ms,
                &mut indexed,
                &mut skipped,
                &mut discovered,
                source.enable_images,
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

/// 按微信 4.x 适配器扫描一个配置的根目录。
#[allow(clippy::too_many_arguments)]
fn scan_wechat_source(
    db: &mut Database,
    device_id: &str,
    root_path: &str,
    scan_started_ms: i64,
    indexed: &mut usize,
    skipped: &mut usize,
    discovered: &mut usize,
    enable_images: bool,
) -> Result<()> {
    let root = PathBuf::from(root_path);
    if !root.is_dir() {
        println!("[-] 微信 4.x 目录不存在，跳过: {}", root_path);
        return Ok(());
    }
    let accounts = WeChat4Detector::find_accounts(&root)?;
    for acc in accounts {
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

        // 图片: msg/attach
        process_scheduled_images(
            db,
            device_id,
            &acc,
            scan_started_ms,
            indexed,
            skipped,
            discovered,
            enable_images,
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
            images_dir: media_root.to_path_buf(),
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

/// 处理聊天图片（定时任务路径）
#[allow(clippy::too_many_arguments)]
fn process_scheduled_images(
    db: &mut Database,
    device_id: &str,
    acc: &adapter_wechat_windows::WeChatAccount,
    scan_started_ms: i64,
    indexed: &mut usize,
    skipped: &mut usize,
    discovered: &mut usize,
    enable_images: bool,
) -> Result<()> {
    use adapter_wechat_windows::media::{discover_image_candidates, prepare_image_candidates};

    let _ = db.recover_pending_image_cache();
    if !enable_images {
        println!("    [image] 已按采集源配置关闭聊天图片解密");
        return Ok(());
    }

    if !acc.images_dir.is_dir() {
        return Ok(());
    }
    let images_root_s = acc.images_dir.to_string_lossy().to_string();
    let candidates = discover_image_candidates(&acc.images_dir, &acc.source_account_id);
    for c in &candidates {
        db.upsert_image_candidate(
            &images_root_s,
            &acc.source_account_id,
            &c.source_path.to_string_lossy(),
            c.file_size as i64,
            c.modified_time.timestamp_millis(),
            c.conv_hash.as_deref(),
            Some(&c.month),
            Some(&c.normalized_stem),
            Some(&c.image_group_key),
        )?;
    }
    *discovered += candidates.len();

    db.recover_image_candidates(Some(&acc.source_account_id))?;
    let pending_rows =
        db.list_pending_image_candidates(&acc.source_account_id, Utc::now().timestamp_millis())?;
    let pending_ids: std::collections::HashMap<String, String> = pending_rows
        .iter()
        .map(|row| (row.source_path.clone(), row.candidate_id.clone()))
        .collect();
    let pending_candidates: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| {
            pending_ids.contains_key(&candidate.source_path.to_string_lossy().to_string())
        })
        .collect();
    for candidate in &pending_candidates {
        if let Some(id) = pending_ids.get(&candidate.source_path.to_string_lossy().to_string()) {
            db.mark_candidate_preparing(id)?;
        }
    }
    let staging = db.staging_dir();
    let batch = prepare_image_candidates(pending_candidates, &acc.source_account_id, &staging, 64);

    let mut all_complete = true;
    for item in &batch.prepared {
        let source_path = item.candidate.source_path.to_string_lossy().to_string();
        let candidate_id = pending_ids.get(&source_path).cloned();
        match db.ingest_prepared_content(&item.prepared, device_id, candidate_id.as_deref()) {
            Ok(IngestResult::Indexed { .. }) => *indexed += 1,
            Ok(IngestResult::Skipped { .. }) => *skipped += 1,
            Err(e) => {
                all_complete = false;
                eprintln!("[-] 图片入库失败: {}", e);
                if let Some(cid) = candidate_id.as_deref() {
                    let _ = db.mark_candidate_failed(
                        cid,
                        chatvault_core::models::ImageErrorCode::PreparedContentMissing,
                    );
                }
            }
        }
    }
    for failure in &batch.failures {
        let source_path = failure.candidate.source_path.to_string_lossy().to_string();
        if let Some(candidate_id) = pending_ids.get(&source_path) {
            let _ = db.mark_candidate_failed(candidate_id, failure.error_code);
        }
    }
    if all_complete {
        db.mark_scan_started(
            &images_root_s,
            WECHAT_WINDOWS_4_SOURCE_TYPE,
            Some(&acc.source_account_id),
            scan_started_ms,
        )?;
    }
    Ok(())
}

/// 按通用目录适配器扫描一个配置的附件目录。
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
