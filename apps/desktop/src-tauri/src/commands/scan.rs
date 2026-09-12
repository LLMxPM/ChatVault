// ChatVault 桌面命令：scan 职责实现与前端错误映射。
// 默认增量：由来源策略发现候选 + 已知文件复检；首次或指定 full_scan 时全量发现。
use super::*;
use chatvault_index::scan_state::system_time_from_ms;
use chatvault_scanner::check_file_stability_sync;
use chrono::Utc;
use std::collections::HashSet;
use std::time::{Duration, SystemTime};

/// 探测本机微信 4.x 账号列表与附件目录
///
/// # 输出
/// - `Result<Vec<WechatAccountDto>, String>`: 微信 4.x 账号及统计概览
#[tauri::command]
pub async fn detect_wechat_accounts() -> std::result::Result<Vec<WechatAccountDto>, String> {
    let root = match WeChat4Detector::detect_root() {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };

    let accounts = WeChat4Detector::find_accounts(&root).unwrap_or_default();
    let mut dtos = Vec::new();

    for acc in accounts {
        let files = WeChat4Parser::parse_account_files(&acc).unwrap_or_default();
        dtos.push(WechatAccountDto {
            source_account_id: acc.source_account_id,
            source_dir: acc.files_dir.to_string_lossy().to_string(),
            files_count_estimated: files.len(),
        });
    }

    Ok(dtos)
}

/// 解析扫描根的增量起点：无检查点或强制全量时返回 None
fn resolve_since(
    db: &chatvault_index::Database,
    root: &str,
    full_scan: bool,
) -> Result<Option<SystemTime>, String> {
    if full_scan {
        return Ok(None);
    }
    db.get_scan_started_ms(root)
        .map_err(|e| e.to_string())
        .map(|ms| ms.map(system_time_from_ms))
}

/// 合并增量遍历结果与已知文件内容变更，按规范化路径去重
fn merge_candidates(
    walked: Vec<chatvault_core::models::DiscoveredFile>,
    changed_known: Vec<chatvault_index::KnownLocalFile>,
    source_type: &str,
    source_account_id: Option<&str>,
    source_conversation_id: Option<String>,
    source_root: Option<&Path>,
) -> Vec<chatvault_core::models::DiscoveredFile> {
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

/// 扫描入库汇总
struct ScanTally {
    discovered: usize,
    new_objects: usize,
    skipped: usize,
}

impl ScanTally {
    fn new() -> Self {
        Self {
            discovered: 0,
            new_objects: 0,
            skipped: 0,
        }
    }
}

/// 入库候选：未变更直接跳过；变更文件先做稳定性检测再 ingest。
///
/// 返回值表示本轮候选是否全部完成处理；false 时调用方不得推进该来源检查点。
fn ingest_candidates(
    db: &mut chatvault_index::Database,
    device_id: &str,
    files: Vec<chatvault_core::models::DiscoveredFile>,
    tally: &mut ScanTally,
) -> Result<bool, String> {
    let mut complete = true;
    tally.discovered += files.len();
    for file in files {
        if db
            .path_is_current(&file.absolute_path)
            .map_err(|e| e.to_string())?
        {
            tally.skipped += 1;
            continue;
        }
        let stable = check_file_stability_sync(&file.absolute_path, Duration::from_millis(50))
            .unwrap_or(false);
        if !stable {
            complete = false;
            continue;
        }
        match db.ingest_file(&file, device_id) {
            Ok(IngestResult::Indexed { is_new_object, .. }) => {
                if is_new_object {
                    tally.new_objects += 1;
                } else {
                    tally.skipped += 1;
                }
            }
            Ok(IngestResult::Skipped { .. }) => {
                tally.skipped += 1;
            }
            Err(e) => {
                complete = false;
                tracing::warn!("入库失败 {}: {}", file.absolute_path, e);
            }
        }
    }
    Ok(complete)
}

/// 触发针对指定微信账号与通用目录的扫描（默认增量，可选全量）
///
/// # 输入
/// - `request`: 账号列表、通用文件夹与 full_scan 标志
/// - `state`: 应用全局上下文
#[tauri::command]
pub async fn run_scan(
    request: ScanRequestDto,
    state: State<'_, AppState>,
) -> std::result::Result<ScanResultDto, String> {
    let start_time = Instant::now();
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    // 使用本轮开始时刻作检查点，扫描过程中新建的文件下轮仍能被发现
    let scan_started_ms = Utc::now().timestamp_millis();
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let mut tally = ScanTally::new();

    // 1. 扫描微信账号
    if let Ok(root) = WeChat4Detector::detect_root() {
        let detected_accounts = WeChat4Detector::find_accounts(&root).unwrap_or_default();
        for acc in detected_accounts {
            if !request.target_accounts.contains(&acc.source_account_id) {
                continue;
            }
            let files_root = acc.files_dir.to_string_lossy().to_string();
            let since = resolve_since(&db, &files_root, request.full_scan)?;
            let walked =
                WeChat4Parser::parse_account_files_since(&acc, since).map_err(|e| e.to_string())?;
            let changed_known = if since.is_some() {
                db.list_changed_known_files(&files_root)
                    .map_err(|e| e.to_string())?
            } else {
                Vec::new()
            };
            let candidates = merge_candidates(
                walked,
                changed_known,
                "wechat-windows-4",
                Some(&acc.source_account_id),
                None,
                Some(&acc.files_dir),
            );
            let complete = ingest_candidates(&mut db, &device_id, candidates, &mut tally)?;
            if complete {
                db.mark_scan_started(
                    &files_root,
                    "wechat-windows-4",
                    Some(&acc.source_account_id),
                    scan_started_ms,
                )
                .map_err(|e| e.to_string())?;
            } else {
                tracing::warn!(
                    "微信账号 {} 本轮存在未完成候选，保留原扫描检查点",
                    acc.source_account_id
                );
            }
        }
    }

    // 2. 扫描通用自定义文件夹
    for folder in &request.custom_folders {
        let p = PathBuf::from(folder);
        if !p.exists() {
            continue;
        }
        let root_s = p.to_string_lossy().to_string();
        let since = resolve_since(&db, &root_s, request.full_scan)?;
        let walked = GenericFolderParser::parse_with_since(&p, since).map_err(|e| e.to_string())?;
        let changed_known = if since.is_some() {
            db.list_changed_known_files(&root_s)
                .map_err(|e| e.to_string())?
        } else {
            Vec::new()
        };
        let candidates =
            merge_candidates(walked, changed_known, "generic-folder", None, None, None);
        let complete = ingest_candidates(&mut db, &device_id, candidates, &mut tally)?;
        if complete {
            db.mark_scan_started(&root_s, "generic-folder", None, scan_started_ms)
                .map_err(|e| e.to_string())?;
        } else {
            tracing::warn!("通用目录 {} 本轮存在未完成候选，保留原扫描检查点", root_s);
        }
    }

    Ok(ScanResultDto {
        total_discovered: tally.discovered,
        total_new_objects: tally.new_objects,
        total_skipped: tally.skipped,
        duration_ms: start_time.elapsed().as_millis(),
    })
}
