// ChatVault 桌面命令：scan 职责实现与前端错误映射。
// 委托 chatvault-scan 共享编排；本层只做 DTO 映射与 Tauri 命令签名。
use super::*;
use chatvault_core::models::CollectSource;
use chatvault_scan::{scan_collect_sources, AccountTarget, ScanEvent, ScanReport, ScanRequest};
use chrono::Utc;

/// 将共享账号信息转为前端契约。
fn to_account_dtos(infos: Vec<chatvault_scan::SourceAccountInfo>) -> Vec<WechatAccountDto> {
    infos
        .into_iter()
        .map(|info| WechatAccountDto {
            source_account_id: info.source_account_id,
            source_dir: info.source_dir,
            source_root: info.source_root,
            files_count_estimated: info.files_count_estimated,
            videos_count_estimated: info.videos_count_estimated,
        })
        .collect()
}

/// 探测本机微信 4.x 账号列表与附件目录
#[tauri::command]
pub async fn detect_wechat_accounts() -> std::result::Result<Vec<WechatAccountDto>, String> {
    let root = match chatvault_scan::detect_wechat_root() {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };
    let infos = chatvault_scan::inspect_wechat_accounts(&root).map_err(|e| e.to_string())?;
    Ok(to_account_dtos(infos))
}

/// 检查用户选择的微信 4.x 根目录并返回账号与附件统计。
#[tauri::command]
pub async fn inspect_wechat_directory(
    path: String,
) -> std::result::Result<Vec<WechatAccountDto>, String> {
    let root = adapter_wechat_windows::WeChat4Detector::validate_root(path.trim())
        .map_err(|e| e.to_string())?;
    let infos = chatvault_scan::inspect_wechat_accounts(&root).map_err(|e| e.to_string())?;
    Ok(to_account_dtos(infos))
}

/// 探测本机企业微信账号列表与附件目录
#[tauri::command]
pub async fn detect_wxwork_accounts() -> std::result::Result<Vec<WechatAccountDto>, String> {
    let root = match chatvault_scan::detect_wxwork_root() {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };
    let infos = chatvault_scan::inspect_wxwork_accounts(&root).map_err(|e| e.to_string())?;
    Ok(to_account_dtos(infos))
}

/// 检查用户选择的企业微信根目录并返回账号与附件统计。
#[tauri::command]
pub async fn inspect_wxwork_directory(
    path: String,
) -> std::result::Result<Vec<WechatAccountDto>, String> {
    let root = adapter_wxwork_windows::WxWorkDetector::validate_root(path.trim())
        .map_err(|e| e.to_string())?;
    let infos = chatvault_scan::inspect_wxwork_accounts(&root).map_err(|e| e.to_string())?;
    Ok(to_account_dtos(infos))
}

/// 将桌面账号 DTO 转为共享编排目标。
fn to_account_targets(items: &[WechatAccountTargetDto]) -> Vec<AccountTarget> {
    items
        .iter()
        .map(|t| AccountTarget {
            source_root: t.source_root.clone(),
            source_account_id: t.source_account_id.clone(),
        })
        .collect()
}

/// 把共享扫描事件映射为 tracing 日志。
fn log_scan_event(event: ScanEvent) {
    match event {
        ScanEvent::SourceStart { source_type, path } => {
            tracing::info!("扫描采集源 {source_type}: {path}");
        }
        ScanEvent::UnknownSourceType { source_type, path } => {
            tracing::warn!("跳过未知采集源类型 {source_type}: {path}");
        }
        ScanEvent::SourceMissing { path, label } => {
            tracing::warn!("{label}: {path}");
        }
        ScanEvent::AccountListed { accounts } => {
            tracing::info!("发现账号 {} 个", accounts.len());
        }
        ScanEvent::AccountSkipped { account_id } => {
            tracing::debug!("跳过未勾选账号 {account_id}");
        }
        ScanEvent::AccountStart { account_id } => {
            tracing::info!("扫描账号 {account_id}");
        }
        ScanEvent::MediaRootStart { kind, path } => {
            tracing::debug!("扫描媒体根 {}: {path}", kind.as_str());
        }
        ScanEvent::MediaRootCandidates { kind, path, count } => {
            tracing::debug!("媒体根 {path} 候选 {count} ({})", kind.as_str());
        }
        ScanEvent::MediaRootIncomplete { path } => {
            tracing::warn!("媒体根 {path} 本轮存在未完成候选，保留原扫描检查点");
        }
        ScanEvent::VideosDisabled { account_id } => {
            tracing::info!("账号 {account_id} 已关闭视频识别");
        }
    }
}

/// 将共享报告映射为前端契约。
fn to_scan_result(report: ScanReport, duration_ms: u128) -> ScanResultDto {
    ScanResultDto {
        total_discovered: report.discovered,
        total_new_objects: report.new_objects,
        total_skipped: report.skipped,
        duration_ms,
    }
}

/// 读取设置中的持久采集源。
fn load_collect_sources(
    db: &chatvault_index::Database,
) -> std::result::Result<Vec<CollectSource>, String> {
    let raw = db
        .get_setting(setting_keys::COLLECT_SOURCES)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "[]".to_string());
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// 执行扫描入库：target_accounts 为 None 时扫描全部账号。
pub(crate) fn execute_scan(
    db: &mut chatvault_index::Database,
    device_id: &str,
    target_accounts: Option<&[WechatAccountTargetDto]>,
    full_scan: bool,
) -> std::result::Result<ScanResultDto, String> {
    let start_time = Instant::now();
    let scan_started_ms = Utc::now().timestamp_millis();
    let sources = load_collect_sources(db)?;
    let targets = target_accounts.map(to_account_targets);
    let req = ScanRequest {
        device_id,
        full_scan,
        target_accounts: targets.as_deref(),
        scan_started_ms,
        should_cancel: None,
        on_event: Some(&log_scan_event),
        on_source_done: None,
    };
    let report = scan_collect_sources(db, &sources, &req).map_err(|e| e.to_string())?;
    Ok(to_scan_result(report, start_time.elapsed().as_millis()))
}

/// 触发针对指定账号与通用目录的扫描（默认增量，可选全量）
#[tauri::command]
pub async fn run_scan(
    request: ScanRequestDto,
    state: State<'_, AppState>,
) -> std::result::Result<ScanResultDto, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    // 空列表表示本次不扫账号型来源，仅扫描通用附件目录。
    let target = if request.target_accounts.is_empty() {
        Some(&[] as &[WechatAccountTargetDto])
    } else {
        Some(request.target_accounts.as_slice())
    };
    execute_scan(&mut db, &device_id, target, request.full_scan)
}
