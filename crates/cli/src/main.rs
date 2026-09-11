//! # ChatVault CLI 命令行验证工具
//!
//! 提供面向终端的实机功能验证、微信 4.x 目录自动探测、批量扫描入库、
//! SQLite+FTS5 中文检索、WebDAV 连通性测试以及端到端归档校验。

use adapter_generic_folder::GenericFolderParser;
use adapter_wechat_windows::{WeChat4Detector, WeChat4Parser};
use anyhow::{Context, Result};
use chatvault_core::models::DiscoveredFile;
use chatvault_index::{Database, IngestResult, SearchFilter, SearchService};
use chatvault_scanner::check_file_stability_sync;
use chatvault_webdav::{CapabilityDetector, WebDavClient, WebDavConfig};
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
        } => handle_scan(&target, &db, &device_id)?,
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
        println!("    {}. 账号标识: {}", i + 1, acc.account_id);
        println!("       根目录:   {}", acc.root_dir.display());
        println!("       附件目录: {}", acc.files_dir.display());

        // 尝试统计当前文件数
        if let Ok(files) = WeChat4Parser::parse_account_files(acc) {
            println!("       当前附件文件数: {}", files.len());
        }
    }

    Ok(())
}

/// 执行扫描并入库 SQLite
///
/// 职责: 收集文件、检查稳定性、计算 BLAKE3、增量去重并建立 FTS5 索引
fn handle_scan(target: &str, db_path: &PathBuf, device_id: &str) -> Result<()> {
    println!("=== 开始执行文件扫描与增量入库 ===");
    let mut files: Vec<DiscoveredFile> = Vec::new();

    if target.eq_ignore_ascii_case("wechat") {
        let root = WeChat4Detector::detect_root().context("探测微信 4.x 根目录失败")?;
        let accounts = WeChat4Detector::find_accounts(&root)?;
        if accounts.is_empty() {
            println!("未找到可扫描的微信 4.x 账号");
            return Ok(());
        }
        for acc in accounts {
            println!("[*] 正在扫描微信账号 [{}] 的附件...", acc.account_id);
            let acc_files = WeChat4Parser::parse_account_files(&acc)?;
            println!("    发现 {} 个候选文件", acc_files.len());
            files.extend(acc_files);
        }
    } else {
        println!("[*] 正在扫描通用目录: {}", target);
        let custom_files = GenericFolderParser::parse(target)?;
        println!("    发现 {} 个候选文件", custom_files.len());
        files.extend(custom_files);
    }

    if files.is_empty() {
        println!("未发现符合归档条件的有效文件");
        return Ok(());
    }

    println!("[*] 正在打开/初始化本地数据库: {}", db_path.display());
    let mut db = Database::open(db_path)?;

    let device_id = resolve_device(&mut db, device_id)?;

    let mut indexed_count = 0;
    let mut skipped_count = 0;
    let mut new_object_count = 0;

    println!("[*] 开始执行稳定性检测与入库...");
    for file in &files {
        // 稳定性校验: 确保文件没有在持续写入中
        let is_stable = check_file_stability_sync(&file.absolute_path, Duration::from_millis(50))
            .unwrap_or(false);
        if !is_stable {
            println!("[-] 跳过处于写入变动中的不稳定文件: {}", file.file_name);
            continue;
        }

        match db.ingest_file(file, &device_id) {
            Ok(IngestResult::Indexed { is_new_object, .. }) => {
                indexed_count += 1;
                if is_new_object {
                    new_object_count += 1;
                }
            }
            Ok(IngestResult::Skipped { .. }) => {
                skipped_count += 1;
            }
            Err(e) => {
                eprintln!("[-] 文件入库异常 {}: {}", file.file_name, e);
            }
        }
    }

    let stats = db.get_stats()?;
    println!("\n=== 入库完成总结 ===");
    println!("  - 本次扫描总数:   {}", files.len());
    println!("  - 本次新入库记录: {}", indexed_count);
    println!("  - 本次新增独立对象: {}", new_object_count);
    println!("  - 幂等跳过未变文件: {}", skipped_count);
    println!("  - 数据库总内容对象: {}", stats.total_objects);
    println!("  - 数据库总来源记录: {}", stats.total_records);
    println!("  - 待上传队列任务数: {}", stats.pending_tasks);

    Ok(())
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
        account_id: account.clone(),
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
        "{:<36} | {:<10} | {:<20} | {}",
        "对象哈希(前12位)", "大小", "时间", "文件名"
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
