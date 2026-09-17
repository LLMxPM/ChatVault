//! # ChatVault CLI 命令行验证工具
//!
//! 提供面向终端的实机功能验证、来源目录自动探测、批量扫描入库、
//! SQLite+FTS5 中文检索、WebDAV 连通性测试以及端到端归档校验。

use anyhow::{Context, Result};
use chatvault_index::{Database, SearchFilter, SearchService};
use chatvault_webdav::{CapabilityDetector, WebDavClient, WebDavConfig};
use clap::Parser;
mod args;
mod logging;
mod scheduled;
mod sync_commands;
use args::{Cli, Commands};
use std::path::PathBuf;
use sync_commands::*;

/// 初始化按命令选择的日志，并确保定时运行的顶层错误也能回看。
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    logging::init(&cli.command)?;
    let result = dispatch(cli).await;
    if let Err(error) = &result {
        tracing::error!(error = %format!("{error:#}"), "CLI 执行失败");
    }
    result
}

/// 分发已解析的命令；业务错误由主入口统一记录并返回非零退出码。
async fn dispatch(cli: Cli) -> Result<()> {
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
        Commands::ScheduledRun { db } => scheduled::run(&db).await?,
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

/// 处理来源目录自动探测命令
///
/// 职责: 查找本机微信 4.x / 企业微信数据目录并输出账号及附件路径
fn handle_detect() -> Result<()> {
    println!("=== 正在探测 Windows 来源数据目录 ===");

    match chatvault_scan::detect_wechat_root() {
        Ok(root) => {
            println!("[+] 检测到微信 4.x 数据根目录: {}", root.display());
            match chatvault_scan::inspect_wechat_accounts(&root) {
                Ok(accounts) if accounts.is_empty() => {
                    println!("    根目录下暂未发现有效微信账号目录");
                }
                Ok(accounts) => {
                    println!("[+] 共发现 {} 个微信账号:", accounts.len());
                    for (i, acc) in accounts.iter().enumerate() {
                        println!("    {}. 账号标识: {}", i + 1, acc.source_account_id);
                        println!("       附件目录: {}", acc.source_dir);
                        println!(
                            "       附件约 {} · 视频约 {}",
                            acc.files_count_estimated, acc.videos_count_estimated
                        );
                    }
                }
                Err(e) => println!("    读取账号失败: {e}"),
            }
        }
        Err(e) => {
            println!("[-] 未检测到微信 4.x: {e}");
        }
    }

    match chatvault_scan::detect_wxwork_root() {
        Ok(root) => {
            println!("[+] 检测到企业微信数据根目录: {}", root.display());
            match chatvault_scan::inspect_wxwork_accounts(&root) {
                Ok(accounts) if accounts.is_empty() => {
                    println!("    根目录下暂未发现有效企业微信账号目录");
                }
                Ok(accounts) => {
                    println!("[+] 共发现 {} 个企业微信账号:", accounts.len());
                    for (i, acc) in accounts.iter().enumerate() {
                        println!("    {}. 账号标识: {}", i + 1, acc.source_account_id);
                        println!("       附件目录: {}", acc.source_dir);
                        println!(
                            "       附件约 {} · 视频约 {}",
                            acc.files_count_estimated, acc.videos_count_estimated
                        );
                    }
                }
                Err(e) => println!("    读取账号失败: {e}"),
            }
        }
        Err(e) => {
            println!("[-] 未检测到企业微信: {e}");
        }
    }

    Ok(())
}

/// 执行扫描并入库 SQLite
///
/// 职责: 委托共享编排收集文件、稳定性检测、哈希去重并建立 FTS5 索引
/// 默认增量；`full` 时忽略检查点全量发现。target 支持 wechat / wxwork /
/// 有效来源根路径或通用附件目录。
fn handle_scan(target: &str, db_path: &PathBuf, device_id: &str, full: bool) -> Result<()> {
    println!(
        "=== 开始执行文件扫描与入库（{}） ===",
        if full { "全量" } else { "增量" }
    );
    println!("[*] 正在打开/初始化本地数据库: {}", db_path.display());
    let mut db = Database::open(db_path)?;
    let device_id = resolve_device(&mut db, device_id)?;
    let report = scan_path_with_shared(&mut db, &device_id, target, full)?;

    if report.discovered == 0 {
        println!("未发现符合归档条件的新增或变更文件");
    }

    let stats = db.get_stats()?;
    println!("\n=== 入库完成总结 ===");
    println!("  - 本次候选总数:   {}", report.discovered);
    println!("  - 本次新入库记录: {}", report.indexed);
    println!("  - 本次新增独立对象: {}", report.new_objects);
    println!("  - 幂等跳过未变文件: {}", report.skipped);
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
        source_type: None,
        source_account_id: account.clone(),
        source_conversation_id: None,
        start_time: None,
        end_time: None,
        limit,
        offset: 0,
        ..Default::default()
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
