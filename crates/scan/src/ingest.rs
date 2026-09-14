// ChatVault 候选合并与入库：目录发现 + 已知文件复检 + 稳定性 + ingest。
use chatvault_core::error::Result;
use chatvault_core::models::DiscoveredFile;
use chatvault_index::scan_state::system_time_from_ms;
use chatvault_index::{Database, IngestResult, KnownLocalFile};
use chatvault_scanner::check_file_stability_sync;
use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// 解析扫描根的增量起点；full_scan 或无检查点时返回 None。
pub fn resolve_since(
    db: &Database,
    root: &str,
    full_scan: bool,
) -> Result<Option<SystemTime>> {
    if full_scan {
        return Ok(None);
    }
    let ms = db.get_scan_started_ms(root)?;
    Ok(ms.map(system_time_from_ms))
}

/// 合并目录遍历结果与已知文件内容变更，按规范化路径去重。
///
/// `resolve_conversation` 对已知文件路径尝试提取会话 ID；无法提取时用调用方
/// 传入的固定 `fallback_conversation_id`（通常为 None）。
pub fn merge_candidates(
    walked: Vec<DiscoveredFile>,
    changed_known: Vec<KnownLocalFile>,
    source_type: &str,
    source_account_id: Option<&str>,
    fallback_conversation_id: Option<String>,
    source_root_for_conversation: Option<&Path>,
    resolve_conversation: Option<&dyn Fn(&Path, &Path) -> Option<String>>,
) -> Vec<DiscoveredFile> {
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
        let conversation_id = source_root_for_conversation
            .zip(resolve_conversation)
            .and_then(|(root, resolve)| {
                resolve(root, Path::new(&known.original_path))
            })
            .or_else(|| fallback_conversation_id.clone());
        if let Some(file) = known.to_discovered(source_type, source_account_id, conversation_id) {
            seen.insert(key);
            merged.push(file);
        }
    }
    merged
}

/// 入库候选：未变更直接跳过；变更文件先做稳定性检测再 ingest。
///
/// 返回值表示本轮候选是否全部完成处理；false 时调用方不得推进该来源检查点。
pub fn ingest_candidates(
    db: &mut Database,
    device_id: &str,
    files: Vec<DiscoveredFile>,
    report: &mut crate::report::ScanReport,
) -> Result<bool> {
    let mut complete = true;
    report.discovered += files.len();
    for file in files {
        if db.path_is_current(&file.absolute_path)? {
            report.skipped += 1;
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
                report.indexed += 1;
                if is_new_object {
                    report.new_objects += 1;
                }
            }
            Ok(IngestResult::Skipped { .. }) => {
                report.skipped += 1;
            }
            Err(e) => {
                complete = false;
                tracing::warn!("入库失败 {}: {}", file.absolute_path, e);
            }
        }
    }
    Ok(complete)
}
