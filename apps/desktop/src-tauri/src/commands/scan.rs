// ChatVault 桌面命令：scan 职责实现与前端错误映射。
// 默认增量：由来源策略发现候选 + 已知文件复检；首次或指定 full_scan 时全量发现。
// 多媒体根：msg/file 与 msg/video 各自独立检查点；图片走候选准备流程。
use super::*;
use adapter_wechat_windows::media::{
    discover_image_candidates, prepare_image_candidates, ImagePrepStats,
};
use chatvault_core::models::{
    CollectSource, GENERIC_FOLDER_SOURCE_TYPE, WECHAT_WINDOWS_4_SOURCE_TYPE,
};
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

    Ok(wechat_account_dtos(&root).unwrap_or_default())
}

/// 检查用户选择的微信 4.x 根目录并返回账号与附件统计。
#[tauri::command]
pub async fn inspect_wechat_directory(
    path: String,
) -> std::result::Result<Vec<WechatAccountDto>, String> {
    let root = WeChat4Detector::validate_root(path.trim()).map_err(|e| e.to_string())?;
    wechat_account_dtos(&root)
}

/// 将指定微信根目录下的账号转换为前端展示对象。
///
/// 职责: 统一自动探测和手动检查的账号 DTO 结构，并保留微信根目录作为
/// 多个微信数据目录同时存在时的定位信息。
fn wechat_account_dtos(
    root: &std::path::Path,
) -> std::result::Result<Vec<WechatAccountDto>, String> {
    let accounts = WeChat4Detector::find_accounts(root).map_err(|e| e.to_string())?;
    let mut dtos = Vec::new();

    for acc in accounts {
        let files = WeChat4Parser::parse_account_files(&acc).unwrap_or_default();
        let videos = WeChat4Parser::parse_account_videos(&acc).unwrap_or_default();
        let images = discover_image_candidates(&acc.images_dir, &acc.source_account_id);
        dtos.push(WechatAccountDto {
            source_account_id: acc.source_account_id,
            source_dir: acc.files_dir.to_string_lossy().to_string(),
            source_root: root.to_string_lossy().to_string(),
            files_count_estimated: files.len(),
            videos_count_estimated: videos.len(),
            images_count_estimated: images.len(),
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

/// 判断微信账号是否属于本次立即扫描目标。
fn is_selected_account(
    target_accounts: Option<&[WechatAccountTargetDto]>,
    root: &std::path::Path,
    account_id: &str,
) -> bool {
    let Some(targets) = target_accounts else {
        return true;
    };
    let root_key = chatvault_core::normalize_scan_key(&root.to_string_lossy());
    targets.iter().any(|target| {
        target.source_account_id == account_id
            && chatvault_core::normalize_scan_key(&target.source_root) == root_key
    })
}

/// 扫描一个微信 4.x 采集源下的全部或指定账号。
///
/// 依次处理 `msg/file`、`msg/video` 两个媒体根（各自独立检查点），
/// 再对 `msg/attach` 图片做候选准备与入库。
#[allow(clippy::too_many_arguments)]
fn scan_wechat_source(
    db: &mut chatvault_index::Database,
    device_id: &str,
    root: &std::path::Path,
    target_accounts: Option<&[WechatAccountTargetDto]>,
    full_scan: bool,
    enable_images: bool,
    scan_started_ms: i64,
    tally: &mut ScanTally,
    image_stats: &mut ImagePrepStats,
) -> std::result::Result<(), String> {
    if !root.is_dir() {
        tracing::warn!("微信 4.x 采集目录不存在，跳过扫描: {}", root.display());
        return Ok(());
    }
    let detected_accounts = WeChat4Detector::find_accounts(root).map_err(|e| e.to_string())?;
    for acc in detected_accounts {
        if !is_selected_account(target_accounts, root, &acc.source_account_id) {
            continue;
        }

        // --- 媒体根 1: msg/file ---
        scan_media_root(
            db,
            device_id,
            &acc.files_dir,
            &acc.source_account_id,
            full_scan,
            scan_started_ms,
            tally,
            true,
        )?;

        // --- 媒体根 2: msg/video ---
        scan_media_root(
            db,
            device_id,
            &acc.video_dir,
            &acc.source_account_id,
            full_scan,
            scan_started_ms,
            tally,
            false,
        )?;

        // --- 图片: msg/attach ---
        process_images(
            db,
            device_id,
            &acc,
            scan_started_ms,
            enable_images,
            tally,
            image_stats,
        )?;
    }
    Ok(())
}

/// 扫描单个媒体根（msg/file 或 msg/video）
///
/// `use_file_parser`: true 用文件解析（含会话目录），false 用视频解析（仅 mp4）
#[allow(clippy::too_many_arguments)]
fn scan_media_root(
    db: &mut chatvault_index::Database,
    device_id: &str,
    media_root: &std::path::Path,
    account_id: &str,
    full_scan: bool,
    scan_started_ms: i64,
    tally: &mut ScanTally,
    use_file_parser: bool,
) -> std::result::Result<(), String> {
    let root_s = media_root.to_string_lossy().to_string();
    let since = resolve_since(db, &root_s, full_scan)?;

    let walked = if use_file_parser {
        // msg/file：复用月份 mtime 裁剪 + 会话目录解析
        if !media_root.exists() {
            return Ok(());
        }
        WeChat4Parser::parse_folder_since(media_root, Some(account_id), since)
            .map_err(|e| e.to_string())?
    } else {
        // msg/video：仅 .mp4
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
        WeChat4Parser::parse_account_videos_since(&fake_account, since)
            .map_err(|e| e.to_string())?
    };

    let changed_known = if since.is_some() {
        db.list_changed_known_files(&root_s)
            .map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    let candidates = merge_candidates(
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
    let complete = ingest_candidates(db, device_id, candidates, tally)?;
    if complete {
        db.mark_scan_started(
            &root_s,
            WECHAT_WINDOWS_4_SOURCE_TYPE,
            Some(account_id),
            scan_started_ms,
        )
        .map_err(|e| e.to_string())?;
    } else {
        tracing::warn!(
            "账号 {} 媒体根 {} 本轮存在未完成候选，保留原扫描检查点",
            account_id,
            root_s
        );
    }
    Ok(())
}

/// 处理账号的聊天图片：候选持久化 → 准备 → 入库
fn process_images(
    db: &mut chatvault_index::Database,
    device_id: &str,
    acc: &adapter_wechat_windows::WeChatAccount,
    scan_started_ms: i64,
    enable_images: bool,
    tally: &mut ScanTally,
    image_stats: &mut ImagePrepStats,
) -> std::result::Result<(), String> {
    let _ = db.recover_pending_image_cache();
    if !enable_images {
        tracing::info!(
            "账号 {} 已关闭聊天图片解密，跳过图片参数与上传",
            acc.source_account_id
        );
        return Ok(());
    }
    if !acc.images_dir.is_dir() {
        return Ok(());
    }
    let images_root_s = acc.images_dir.to_string_lossy().to_string();

    // 1. 发现候选并持久化
    let candidates = discover_image_candidates(&acc.images_dir, &acc.source_account_id);
    let discovered_count = candidates.len();
    let logical_groups = candidates
        .iter()
        .map(|candidate| candidate.image_group_key.clone())
        .collect::<std::collections::HashSet<_>>()
        .len();
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
        )
        .map_err(|e| e.to_string())?;
    }

    db.recover_image_candidates(Some(&acc.source_account_id))
        .map_err(|e| e.to_string())?;
    let pending_rows = db
        .list_pending_image_candidates(&acc.source_account_id, Utc::now().timestamp_millis())
        .map_err(|e| e.to_string())?;
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
        if let Some(candidate_id) =
            pending_ids.get(&candidate.source_path.to_string_lossy().to_string())
        {
            db.mark_candidate_preparing(candidate_id)
                .map_err(|e| e.to_string())?;
        }
    }

    // 2. 准备（解密 + 校验 + 暂存）
    let staging = db.staging_dir();
    let batch = prepare_image_candidates(
        pending_candidates,
        &acc.source_account_id,
        &staging,
        64, // 候选密钥上限
    );
    image_stats.discovered += discovered_count;
    image_stats.prepared += batch.stats.prepared;
    image_stats.parameters_unavailable += batch.stats.parameters_unavailable;
    image_stats.parameters_not_applicable += batch.stats.parameters_not_applicable;
    image_stats.unsupported_structure += batch.stats.unsupported_structure;
    image_stats.unsupported_payload += batch.stats.unsupported_payload;
    image_stats.invalid_image += batch.stats.invalid_image;
    image_stats.waiting_stable += batch.stats.waiting_stable;
    image_stats.logical_groups += logical_groups;

    tally.discovered += discovered_count;

    // 3. 入库成功的明文
    let mut all_complete = true;
    for item in &batch.prepared {
        let source_path = item.candidate.source_path.to_string_lossy().to_string();
        // 找到候选 ID
        let candidate_id = pending_ids.get(&source_path).cloned();

        match db.ingest_prepared_content(&item.prepared, device_id, candidate_id.as_deref()) {
            Ok(chatvault_index::IngestResult::Indexed { is_new_object, .. }) => {
                if is_new_object {
                    tally.new_objects += 1;
                } else {
                    tally.skipped += 1;
                }
            }
            Ok(chatvault_index::IngestResult::Skipped { .. }) => {
                tally.skipped += 1;
            }
            Err(e) => {
                all_complete = false;
                tracing::warn!("图片入库失败 {}: {}", source_path, e);
                if let Some(cid) = candidate_id.as_deref() {
                    let _ = db.mark_candidate_failed(
                        cid,
                        chatvault_core::models::ImageErrorCode::PreparedContentMissing,
                    );
                }
            }
        }
    }

    // 4. 标记失败候选
    for failure in &batch.failures {
        let source_path = failure.candidate.source_path.to_string_lossy().to_string();
        if let Some(candidate_id) = pending_ids.get(&source_path) {
            let _ = db.mark_candidate_failed(candidate_id, failure.error_code);
        }
    }

    // 5. 目录枚举完成且候选落库后推进图片根检查点
    // 注意：检查点只表示「已发现」，不代表「全部成功备份」
    if all_complete {
        db.mark_scan_started(
            &images_root_s,
            WECHAT_WINDOWS_4_SOURCE_TYPE,
            Some(&acc.source_account_id),
            scan_started_ms,
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// 扫描一个通用附件目录采集源。
fn scan_generic_source(
    db: &mut chatvault_index::Database,
    device_id: &str,
    root: &std::path::Path,
    full_scan: bool,
    scan_started_ms: i64,
    tally: &mut ScanTally,
) -> std::result::Result<(), String> {
    if !root.is_dir() {
        tracing::warn!("附件采集目录不存在，跳过扫描: {}", root.display());
        return Ok(());
    }
    let root_s = root.to_string_lossy().to_string();
    let since = resolve_since(db, &root_s, full_scan)?;
    let walked = GenericFolderParser::parse_with_since(root, since).map_err(|e| e.to_string())?;
    let changed_known = if since.is_some() {
        db.list_changed_known_files(&root_s)
            .map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    let candidates = merge_candidates(
        walked,
        changed_known,
        GENERIC_FOLDER_SOURCE_TYPE,
        None,
        None,
        None,
    );
    let complete = ingest_candidates(db, device_id, candidates, tally)?;
    if complete {
        db.mark_scan_started(&root_s, GENERIC_FOLDER_SOURCE_TYPE, None, scan_started_ms)
            .map_err(|e| e.to_string())?;
    } else {
        tracing::warn!("通用目录 {} 本轮存在未完成候选，保留原扫描检查点", root_s);
    }
    Ok(())
}

/// 执行扫描入库：target_accounts 为 None 时扫描全部微信账号。
///
/// 采集源配置携带适配器类型，配置列表是实际扫描范围。
pub(crate) fn execute_scan(
    db: &mut chatvault_index::Database,
    device_id: &str,
    target_accounts: Option<&[WechatAccountTargetDto]>,
    full_scan: bool,
) -> std::result::Result<ScanResultDto, String> {
    let start_time = Instant::now();
    let scan_started_ms = Utc::now().timestamp_millis();
    let mut tally = ScanTally::new();
    let mut image_stats = ImagePrepStats::default();
    let sources = load_collect_sources(db)?;

    for source in &sources {
        let root = PathBuf::from(&source.path);
        match source.source_type.as_str() {
            WECHAT_WINDOWS_4_SOURCE_TYPE => scan_wechat_source(
                db,
                device_id,
                &root,
                target_accounts,
                full_scan,
                source.enable_images,
                scan_started_ms,
                &mut tally,
                &mut image_stats,
            )?,
            GENERIC_FOLDER_SOURCE_TYPE => {
                scan_generic_source(db, device_id, &root, full_scan, scan_started_ms, &mut tally)?
            }
            other => tracing::warn!("跳过未知采集源类型 {}: {}", other, source.path),
        }
    }

    Ok(ScanResultDto {
        total_discovered: tally.discovered,
        total_new_objects: tally.new_objects,
        total_skipped: tally.skipped,
        duration_ms: start_time.elapsed().as_millis(),
        images_discovered: image_stats.discovered,
        images_prepared: image_stats.prepared,
        images_parameters_unavailable: image_stats.parameters_unavailable,
        images_unsupported: image_stats.unsupported_structure + image_stats.unsupported_payload,
        images_invalid: image_stats.invalid_image,
        images_logical_groups: image_stats.logical_groups,
    })
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

/// 触发针对指定微信账号与通用目录的扫描（默认增量，可选全量）
///
/// # 输入
/// - `request`: 账号列表（空则跳过微信）与 full_scan 标志
/// - `state`: 应用全局上下文
#[tauri::command]
pub async fn run_scan(
    request: ScanRequestDto,
    state: State<'_, AppState>,
) -> std::result::Result<ScanResultDto, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    // 空列表表示本次不扫微信，仅扫描通用附件目录。
    let target = if request.target_accounts.is_empty() {
        Some(&[] as &[WechatAccountTargetDto])
    } else {
        Some(request.target_accounts.as_slice())
    };
    execute_scan(&mut db, &device_id, target, request.full_scan)
}
