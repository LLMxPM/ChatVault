// ChatVault CLI 元数据同步、恢复及手动扫描入口。
use super::*;
use chatvault_core::models::{
    CollectSource, WECHAT_WINDOWS_4_SOURCE_TYPE, WXWORK_WINDOWS_SOURCE_TYPE,
};
use chatvault_scan::{
    scan_generic_source, scan_wechat_source, scan_wxwork_source, ScanEvent, ScanReport, ScanRequest,
};
use chrono::Utc;

/// 打印共享扫描事件。
pub(crate) fn print_scan_event(event: &ScanEvent) {
    match event {
        ScanEvent::SourceStart { source_type, path } => {
            tracing::info!("[*] 扫描采集源 {source_type}: {path}");
        }
        ScanEvent::UnknownSourceType { source_type, path } => {
            tracing::info!("[-] 跳过未知采集源类型 {source_type}: {path}");
        }
        ScanEvent::SourceMissing { path, label } => {
            tracing::info!("[-] {label}: {path}");
        }
        ScanEvent::AccountListed { accounts } => {
            tracing::info!("[+] 发现账号 {} 个", accounts.len());
        }
        ScanEvent::AccountSkipped { account_id } => {
            tracing::info!("[*] 跳过未勾选账号 [{account_id}]");
        }
        ScanEvent::AccountStart { account_id } => {
            tracing::info!("[*] 扫描账号 [{account_id}]");
        }
        ScanEvent::MediaRootStart { kind, path } => {
            tracing::info!("    [{}] {}", kind.as_str(), path);
        }
        ScanEvent::MediaRootCandidates { kind, path, count } => {
            tracing::info!("    [{}] 候选 {} 个文件 ({path})", kind.as_str(), count);
        }
        ScanEvent::MediaRootIncomplete { path } => {
            tracing::info!("[-] 媒体根 {path} 存在未完成候选，保留原扫描检查点");
        }
        ScanEvent::VideosDisabled { account_id } => {
            tracing::info!("    [video] 账号 {account_id} 已按采集源配置关闭视频识别");
        }
    }
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
