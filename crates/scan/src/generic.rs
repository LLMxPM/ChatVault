// ChatVault 通用附件目录扫描编排。
use crate::events::ScanEvent;
use crate::ingest::{ingest_candidates, merge_candidates, resolve_since};
use crate::report::{ScanReport, ScanRequest};
use adapter_generic_folder::GenericFolderParser;
use chatvault_core::error::Result;
use chatvault_core::models::GENERIC_FOLDER_SOURCE_TYPE;
use chatvault_index::Database;
use std::path::Path;

/// 扫描一个通用附件目录采集源。
pub fn scan_generic_source(
    db: &mut Database,
    root: &Path,
    req: &ScanRequest<'_>,
) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    if !root.is_dir() {
        req.emit(ScanEvent::SourceMissing {
            path: root.display().to_string(),
            label: "附件采集目录不存在，跳过扫描",
        });
        return Ok(report);
    }

    let root_s = root.to_string_lossy().to_string();
    let since = resolve_since(db, &root_s, req.full_scan)?;
    let walked = GenericFolderParser::parse_with_since(root, since)?;
    let changed_known = if since.is_some() {
        db.list_changed_known_files(&root_s)?
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
        None,
    );
    req.emit(ScanEvent::MediaRootCandidates {
        kind: crate::events::MediaKind::Files,
        path: root_s.clone(),
        count: candidates.len(),
    });

    let complete = ingest_candidates(db, req.device_id, candidates, &mut report)?;
    if complete {
        db.mark_scan_started(
            &root_s,
            GENERIC_FOLDER_SOURCE_TYPE,
            None,
            req.scan_started_ms,
        )?;
    } else {
        req.emit(ScanEvent::MediaRootIncomplete { path: root_s });
    }
    Ok(report)
}
