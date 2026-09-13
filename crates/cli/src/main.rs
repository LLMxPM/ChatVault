//! # ChatVault CLI 命令行验证工具
//!
//! 提供面向终端的实机功能验证、微信 4.x 目录自动探测、批量扫描入库、
//! SQLite+FTS5 中文检索、WebDAV 连通性测试以及端到端归档校验。

use adapter_generic_folder::GenericFolderParser;
use adapter_wechat_windows::{WeChat4Detector, WeChat4Parser};
use anyhow::{Context, Result};
use chatvault_core::models::{CollectSource, DiscoveredFile, WECHAT_WINDOWS_4_SOURCE_TYPE};
use chatvault_index::{Database, IngestResult, SearchFilter, SearchService};
use chatvault_scanner::check_file_stability_sync;
use chatvault_webdav::{CapabilityDetector, WebDavClient, WebDavConfig};
use chrono::Utc;
use clap::Parser;
mod args;
mod sync_commands;
use args::{Cli, Commands};
use std::path::PathBuf;
use std::time::Duration;
use sync_commands::*;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Detect => handle_detect()?,
        Commands::Scan {
            target,
            db,
            device_id,
            full,
        } => handle_scan(&target, &db, &device_id, full)?,
        Commands::Search {
            keyword,
            ext,
            account,
            limit,
            db,
        } => handle_search(keyword, ext, account, limit, &db)?,
        Commands::WebdavTest { url, user, pass } => handle_webdav_test(&url, user, pass).await?,
        Commands::Archive {
            url,
            user,
            pass,
            vault_id,
            db,
            limit,
        } => handle_archive(&url, user, pass, &vault_id, &db, limit).await?,
        Commands::ScheduledRun { db } => handle_scheduled_run(&db).await?,
        Commands::SyncPublish {
            url,
            user,
            pass,
            vault_id,
            db,
            device_id,
        } => handle_sync_publish(&url, user, pass, &vault_id, &db, &device_id).await?,
        Commands::SyncPull {
            url,
            user,
            pass,
            vault_id,
            db,
            device_id,
        } => handle_sync_pull(&url, user, pass, &vault_id, &db, &device_id).await?,
        Commands::Restore {
            url,
            user,
            pass,
            vault_id,
            db,
            device_id,
        } => handle_restore(&url, user, pass, &vault_id, &db, &device_id).await?,
    }

    Ok(())
}

/// 处理微信 4.x 目录自动探测命令
///
/// 职责: 查找本地 xwechat_files 目录并输出账号及附件路径
fn handle_detect() -> Result<()> {
    println!("=== 正在探测 Windows 微信 4.x 数据目录 ===");
    let root = match WeChat4Detector::detect_root() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[-] 探测失败: {}", e);
            eprintln!("提示: 请确认当前机器已安装并登录过微信 4.x (数据目录 xwechat_files)");
            return Ok(());
        }
    };

    println!("[+] 成功检测到微信 4.x 数据根目录: {}", root.display());

    let accounts = WeChat4Detector::find_accounts(&root)?;
    if accounts.is_empty() {
        println!("[-] 根目录下暂未发现有效微信账号目录");
        return Ok(());
    }

    println!("[+] 共发现 {} 个微信账号:", accounts.len());
    for (i, acc) in accounts.iter().enumerate() {
        println!("    {}. 账号标识: {}", i + 1, acc.source_account_id);
        println!("       根目录:   {}", acc.root_dir.display());
        println!("       附件目录: {}", acc.files_dir.display());
        println!("       视频目录: {}", acc.video_dir.display());
        println!("       图片目录: {}", acc.images_dir.display());

        // 尝试统计当前文件数
        if let Ok(files) = WeChat4Parser::parse_account_files(acc) {
            println!("       当前附件文件数: {}", files.len());
        }
        if let Ok(videos) = WeChat4Parser::parse_account_videos(acc) {
            println!("       当前视频文件数: {}", videos.len());
        }
    }

    Ok(())
}

/// 执行扫描并入库 SQLite
///
/// 职责: 收集文件、检查稳定性、计算 BLAKE3、增量去重并建立 FTS5 索引
/// 默认增量（由来源策略发现候选 + 已知文件复检）；`full` 时忽略检查点全量发现
fn handle_scan(target: &str, db_path: &PathBuf, device_id: &str, full: bool) -> Result<()> {
    println!(
        "=== 开始执行文件扫描与入库（{}） ===",
        if full { "全量" } else { "增量" }
    );
    println!("[*] 正在打开/初始化本地数据库: {}", db_path.display());
    let mut db = Database::open(db_path)?;
    let device_id = resolve_device(&mut db, device_id)?;
    let scan_started_ms = Utc::now().timestamp_millis();

    let mut indexed_count = 0;
    let mut skipped_count = 0;
    let mut new_object_count = 0;
    let mut discovered_count = 0;

    let wechat_root = if target.eq_ignore_ascii_case("wechat") {
        Some(WeChat4Detector::detect_root().context("探测微信 4.x 根目录失败")?)
    } else {
        WeChat4Detector::validate_root(target).ok()
    };
    if let Some(root) = wechat_root {
        let configured_images = load_cli_image_setting(&db, &root.to_string_lossy());
        let accounts = WeChat4Detector::find_accounts(&root)?;
        if accounts.is_empty() {
            println!("未找到可扫描的微信 4.x 账号");
            return Ok(());
        }
        for acc in accounts {
            println!("[*] 正在扫描微信账号 [{}]...", acc.source_account_id);

            // 媒体根 1: msg/file
            scan_cli_media_root(
                &mut db,
                &device_id,
                &acc.files_dir,
                &acc.source_account_id,
                full,
                scan_started_ms,
                &mut indexed_count,
                &mut skipped_count,
                &mut new_object_count,
                &mut discovered_count,
                true,
            )?;

            // 媒体根 2: msg/video（仅 .mp4）
            scan_cli_media_root(
                &mut db,
                &device_id,
                &acc.video_dir,
                &acc.source_account_id,
                full,
                scan_started_ms,
                &mut indexed_count,
                &mut skipped_count,
                &mut new_object_count,
                &mut discovered_count,
                false,
            )?;

            // 图片: msg/attach
            process_cli_images(
                &mut db,
                &device_id,
                &acc,
                full,
                scan_started_ms,
                &mut indexed_count,
                &mut skipped_count,
                &mut new_object_count,
                &mut discovered_count,
                configured_images,
            )?;
        }
    } else {
        println!("[*] 正在扫描通用目录: {}", target);
        let since = resolve_scan_since(&db, target, full)?;
        let walked = GenericFolderParser::parse_with_since(target, since)?;
        let changed_known = if since.is_some() {
            db.list_changed_known_files(target)?
        } else {
            Vec::new()
        };
        let files = merge_changed_known(walked, changed_known, "generic-folder", None, None, None);
        println!("    本次候选 {} 个文件", files.len());
        discovered_count += files.len();
        let complete = process_scan_files(
            &mut db,
            &device_id,
            &files,
            &mut indexed_count,
            &mut skipped_count,
            &mut new_object_count,
        )?;
        if complete {
            db.mark_scan_started(target, "generic-folder", None, scan_started_ms)?;
        } else {
            println!("[-] 通用目录存在未完成候选，保留原扫描检查点");
        }
    }

    if discovered_count == 0 {
        println!("未发现符合归档条件的新增或变更文件");
    }

    let stats = db.get_stats()?;
    println!("\n=== 入库完成总结 ===");
    println!("  - 本次候选总数:   {}", discovered_count);
    println!("  - 本次新入库记录: {}", indexed_count);
    println!("  - 本次新增独立对象: {}", new_object_count);
    println!("  - 幂等跳过未变文件: {}", skipped_count);
    println!("  - 数据库总内容对象: {}", stats.total_objects);
    println!("  - 数据库总来源记录: {}", stats.total_records);
    println!("  - 待上传队列任务数: {}", stats.pending_tasks);

    Ok(())
}

/// 解析扫描根增量起点
fn resolve_scan_since(
    db: &Database,
    root: &str,
    full: bool,
) -> Result<Option<std::time::SystemTime>> {
    if full {
        return Ok(None);
    }
    let ms = db.get_scan_started_ms(root)?;
    Ok(ms.map(chatvault_index::system_time_from_ms))
}

/// 从持久化采集源配置读取微信图片开关；直接扫描未配置路径时默认开启。
fn load_cli_image_setting(db: &Database, root: &str) -> bool {
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
            source.source_type == WECHAT_WINDOWS_4_SOURCE_TYPE
                && chatvault_core::normalize_scan_key(&source.path) == key
        })
        .map(|source| source.enable_images)
        .unwrap_or(true)
}

/// 扫描单个媒体根（msg/file 或 msg/video）
#[allow(clippy::too_many_arguments)]
fn scan_cli_media_root(
    db: &mut Database,
    device_id: &str,
    media_root: &std::path::Path,
    account_id: &str,
    full: bool,
    scan_started_ms: i64,
    indexed_count: &mut usize,
    skipped_count: &mut usize,
    new_object_count: &mut usize,
    discovered_count: &mut usize,
    use_file_parser: bool,
) -> Result<()> {
    let root_s = media_root.to_string_lossy().to_string();
    let since = resolve_scan_since(db, &root_s, full)?;

    let walked = if use_file_parser {
        if !media_root.exists() {
            return Ok(());
        }
        WeChat4Parser::parse_folder_since(media_root, Some(account_id), since)?
    } else {
        let fake_account = adapter_wechat_windows::WeChatAccount {
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
        WeChat4Parser::parse_account_videos_since(&fake_account, since)?
    };

    let changed_known = if since.is_some() {
        db.list_changed_known_files(&root_s)?
    } else {
        Vec::new()
    };
    let files = merge_changed_known(
        walked,
        changed_known,
        "wechat-windows-4",
        Some(account_id),
        None,
        if use_file_parser {
            Some(media_root)
        } else {
            None
        },
    );
    println!(
        "    [{}] 本次候选 {} 个文件",
        if use_file_parser { "file" } else { "video" },
        files.len()
    );
    *discovered_count += files.len();
    let complete = process_scan_files(
        db,
        device_id,
        &files,
        indexed_count,
        skipped_count,
        new_object_count,
    )?;
    if complete {
        db.mark_scan_started(
            &root_s,
            "wechat-windows-4",
            Some(account_id),
            scan_started_ms,
        )?;
    } else {
        println!("[-] 媒体根 {} 存在未完成候选，保留原扫描检查点", root_s);
    }
    Ok(())
}

/// 处理账号的聊天图片
///
/// `full` 为 true 时忽略检查点全量发现；失败重试始终由候选队列驱动。
#[allow(clippy::too_many_arguments)]
fn process_cli_images(
    db: &mut Database,
    device_id: &str,
    acc: &adapter_wechat_windows::WeChatAccount,
    full: bool,
    scan_started_ms: i64,
    indexed_count: &mut usize,
    skipped_count: &mut usize,
    new_object_count: &mut usize,
    discovered_count: &mut usize,
    enable_images: bool,
) -> Result<()> {
    use adapter_wechat_windows::media::{
        discover_image_candidates, image_candidate_from_disk, image_candidate_from_stored,
        prepare_image_candidates, ImageCandidate,
    };

    let _ = db.recover_pending_image_cache();
    if !enable_images {
        println!("    [image] 已按采集源配置关闭聊天图片解密");
        return Ok(());
    }

    if !acc.images_dir.is_dir() {
        return Ok(());
    }
    let images_root_s = acc.images_dir.to_string_lossy().to_string();
    let since = resolve_scan_since(db, &images_root_s, full)?;
    let mut to_upsert: Vec<ImageCandidate> =
        discover_image_candidates(&acc.images_dir, &acc.source_account_id, since);
    if since.is_some() {
        for known in db.list_changed_known_files(&images_root_s)? {
            if let Some(candidate) = image_candidate_from_disk(
                &acc.images_dir,
                &acc.source_account_id,
                &std::path::Path::new(&known.original_path),
            ) {
                let key =
                    chatvault_core::normalize_scan_key(&candidate.source_path.to_string_lossy());
                let already = to_upsert.iter().any(|c| {
                    chatvault_core::normalize_scan_key(&c.source_path.to_string_lossy()) == key
                });
                if !already {
                    to_upsert.push(candidate);
                }
            }
        }
    }
    for c in &to_upsert {
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
    println!("    [image] 本次候选 {} 个图片", to_upsert.len());
    *discovered_count += to_upsert.len();
    db.mark_scan_started(
        &images_root_s,
        "wechat-windows-4",
        Some(&acc.source_account_id),
        scan_started_ms,
    )?;

    db.recover_image_candidates(Some(&acc.source_account_id))?;
    let pending_rows =
        db.list_pending_image_candidates(&acc.source_account_id, Utc::now().timestamp_millis())?;
    let mut pending_candidates: Vec<ImageCandidate> = Vec::new();
    let mut pending_ids: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for row in &pending_rows {
        let Some(candidate) = image_candidate_from_stored(
            &row.source_path,
            row.source_size,
            row.source_mtime_ms,
            row.conv_hash.as_deref(),
            row.month.as_deref(),
            row.normalized_stem.as_deref(),
            row.image_group_key.as_deref(),
        ) else {
            continue;
        };
        if !candidate.source_path.is_file() {
            continue;
        }
        pending_ids.insert(row.source_path.clone(), row.candidate_id.clone());
        pending_candidates.push(candidate);
    }
    for candidate in &pending_candidates {
        let path = candidate.source_path.to_string_lossy().to_string();
        if let Some(id) = pending_ids.get(&path) {
            db.mark_candidate_preparing(id)?;
        }
    }

    let staging = db.staging_dir();
    let batch = prepare_image_candidates(pending_candidates, &acc.source_account_id, &staging, 64);

    for item in &batch.prepared {
        let source_path = item.candidate.source_path.to_string_lossy().to_string();
        let candidate_id = pending_ids.get(&source_path).cloned();

        match db.ingest_prepared_content(&item.prepared, device_id, candidate_id.as_deref()) {
            Ok(IngestResult::Indexed { is_new_object, .. }) => {
                *indexed_count += 1;
                if is_new_object {
                    *new_object_count += 1;
                }
            }
            Ok(IngestResult::Skipped { .. }) => {
                *skipped_count += 1;
            }
            Err(e) => {
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

    println!(
        "    [image] 解密成功 {}, 参数不可用 {}, 未支持 {}, 校验失败 {}, 等待稳定 {}",
        batch.stats.prepared,
        batch.stats.parameters_unavailable,
        batch.stats.unsupported_structure + batch.stats.unsupported_payload,
        batch.stats.invalid_image,
        batch.stats.waiting_stable
    );

    Ok(())
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
fn process_scan_files(
    db: &mut Database,
    device_id: &str,
    files: &[DiscoveredFile],
    indexed_count: &mut usize,
    skipped_count: &mut usize,
    new_object_count: &mut usize,
) -> Result<bool> {
    let mut complete = true;
    for file in files {
        if db.path_is_current(&file.absolute_path)? {
            *skipped_count += 1;
            continue;
        }
        let is_stable = check_file_stability_sync(&file.absolute_path, Duration::from_millis(50))
            .unwrap_or(false);
        if !is_stable {
            complete = false;
            println!("[-] 跳过处于写入变动中的不稳定文件: {}", file.file_name);
            continue;
        }
        match db.ingest_file(file, device_id) {
            Ok(IngestResult::Indexed { is_new_object, .. }) => {
                *indexed_count += 1;
                if is_new_object {
                    *new_object_count += 1;
                }
            }
            Ok(IngestResult::Skipped { .. }) => {
                *skipped_count += 1;
            }
            Err(e) => {
                complete = false;
                eprintln!("[-] 文件入库异常 {}: {}", file.file_name, e);
            }
        }
    }
    Ok(complete)
}

/// 执行多维检索
///
/// 职责: 调用 SearchService 并将结果格式化展示在控制台
fn handle_search(
    keyword: Option<String>,
    ext: Option<String>,
    account: Option<String>,
    limit: usize,
    db_path: &PathBuf,
) -> Result<()> {
    if !db_path.exists() {
        println!("数据库文件不存在: {}", db_path.display());
        return Ok(());
    }

    let db = Database::open(db_path)?;
    let service = SearchService::new(&db);

    let filter = SearchFilter {
        keyword: keyword.clone(),
        extension: ext.clone(),
        category: None,
        source_type: None,
        source_account_id: account.clone(),
        source_conversation_id: None,
        start_time: None,
        end_time: None,
        limit,
        offset: 0,
    };

    println!(
        "=== 执行检索 [关键词: {:?}, 扩展名: {:?}, 账号: {:?}] ===",
        keyword.as_deref().unwrap_or("*"),
        ext.as_deref().unwrap_or("*"),
        account.as_deref().unwrap_or("*")
    );

    let results = service.search(&filter)?;
    if results.is_empty() {
        println!("没有找到匹配的文件记录");
        return Ok(());
    }

    println!("[+] 命中 {} 条记录:\n", results.len());
    println!(
        "{:<36} | {:<10} | {:<20} | 文件名",
        "对象哈希(前12位)", "大小", "时间"
    );
    println!("{:-<100}", "");

    for item in results {
        let size_kb = item.size as f64 / 1024.0;
        let size_str = if size_kb > 1024.0 {
            format!("{:.2} MB", size_kb / 1024.0)
        } else {
            format!("{:.1} KB", size_kb)
        };
        let hash_prefix = if item.object_id.len() > 18 {
            &item.object_id[..18]
        } else {
            &item.object_id
        };

        println!(
            "{:<36} | {:<10} | {:<20} | {}",
            hash_prefix,
            size_str,
            &item.file_time[..item.file_time.len().min(19)],
            item.original_name
        );
        if let Some(path) = item.original_path {
            println!("    └─ 本地路径: {}", path);
        }
    }

    Ok(())
}

/// 执行 WebDAV 服务能力探测
async fn handle_webdav_test(url: &str, user: Option<String>, pass: Option<String>) -> Result<()> {
    println!("=== 正在探测 WebDAV 服务器能力 ===");
    println!("目标 URL: {}", url);

    let config = WebDavConfig {
        base_url: url.to_string(),
        username: user,
        password: pass,
    };

    let client = WebDavClient::new(config)?;
    let detector = CapabilityDetector::new(&client);
    let report = detector.detect().await?;

    println!("\n=== WebDAV 能力探测报告 ===");
    println!(
        "  - 网络连通性:     {}",
        if report.reachable {
            "正常"
        } else {
            "不可达"
        }
    );
    println!(
        "  - 账号认证:       {}",
        if report.authenticated {
            "成功"
        } else {
            "失败"
        }
    );
    println!(
        "  - MKCOL 创建目录: {}",
        if report.support_mkcol {
            "支持"
        } else {
            "不支持"
        }
    );
    println!(
        "  - MOVE 暂存移动:  {}",
        if report.support_move {
            "支持"
        } else {
            "不支持"
        }
    );
    println!("  - 诊断说明:       {}", report.message);

    Ok(())
}

/// 执行归档上传与远端回读校验闭环
async fn handle_archive(
    url: &str,
    user: Option<String>,
    pass: Option<String>,
    vault_id: &str,
    db_path: &PathBuf,
    limit: usize,
) -> Result<()> {
    let mut db = Database::open(db_path)?;
    let device_id = db.ensure_device_identity()?;
    let client = WebDavClient::new(WebDavConfig {
        base_url: url.into(),
        username: user,
        password: pass,
    })?;
    let result =
        chatvault_sync::archive::archive_pending(&client, &mut db, vault_id, &device_id, limit)
            .await?;
    println!(
        "归档完成：新上传 {}，已校验 {}，失败 {}",
        result.uploaded, result.verified, result.failed
    );
    if result.failed > 0 {
        anyhow::bail!("存在归档失败任务，已保存失败原因并等待重试");
    }
    Ok(())
}

/// 首次允许指定设备标识，后续使用持久化身份，避免扫描和同步命令各用不同默认值。
fn resolve_device(db: &mut Database, requested: &str) -> Result<String> {
    if !requested.is_empty() {
        chatvault_metadata::validate_id(requested)?;
        if let Some(current) = db.get_setting("device_id")? {
            if current != requested {
                anyhow::bail!("设备身份已经绑定，不能通过命令参数更改");
            }
        } else {
            db.set_setting("device_id", requested)?;
        }
    }
    Ok(db.ensure_device_identity()?)
}
