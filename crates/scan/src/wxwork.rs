// ChatVault 企业微信 Windows 采集源扫描编排。
use crate::events::{MediaKind, ScanEvent};
use crate::ingest::{ingest_candidates, merge_candidates, resolve_since};
use crate::report::{account_selected, ScanReport, ScanRequest};
use adapter_wxwork_windows::{WxWorkAccount, WxWorkDetector, WxWorkParser};
use chatvault_core::error::Result;
use chatvault_core::models::WXWORK_WINDOWS_SOURCE_TYPE;
use chatvault_index::Database;
use std::path::{Path, PathBuf};

/// 扫描一个企业微信根目录下的全部或勾选账号。
pub fn scan_wxwork_source(
    db: &mut Database,
    root: &Path,
    enable_videos: bool,
    req: &ScanRequest<'_>,
) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    if !root.is_dir() {
        req.emit(ScanEvent::SourceMissing {
            path: root.display().to_string(),
            label: "企业微信采集目录不存在，跳过扫描",
        });
        return Ok(report);
    }
    let accounts = WxWorkDetector::find_accounts(root)?;
    for acc in accounts {
        if req.cancelled() {
            return Ok(report);
        }
        if !account_selected(req.target_accounts, root, &acc.source_account_id) {
            req.emit(ScanEvent::AccountSkipped {
                account_id: acc.source_account_id,
            });
            continue;
        }
        req.emit(ScanEvent::AccountStart {
            account_id: acc.source_account_id.clone(),
        });

        scan_media_root(
            db,
            &acc.files_dir,
            Some(&acc.source_account_id),
            MediaKind::Files,
            req,
            &mut report,
        )?;

        if !enable_videos {
            req.emit(ScanEvent::VideosDisabled {
                account_id: acc.source_account_id.clone(),
            });
            continue;
        }
        scan_media_root(
            db,
            &acc.video_dir,
            Some(&acc.source_account_id),
            MediaKind::Videos,
            req,
            &mut report,
        )?;
    }
    Ok(report)
}

/// 扫描企业微信单个媒体根（Cache/File 或 Cache/Video）。
fn scan_media_root(
    db: &mut Database,
    media_root: &Path,
    account_id: Option<&str>,
    kind: MediaKind,
    req: &ScanRequest<'_>,
    report: &mut ScanReport,
) -> Result<()> {
    if matches!(kind, MediaKind::Files) && !media_root.exists() {
        return Ok(());
    }
    req.emit(ScanEvent::MediaRootStart {
        kind,
        path: media_root.display().to_string(),
    });

    let root_s = media_root.to_string_lossy().to_string();
    let since = resolve_since(db, &root_s, req.full_scan)?;
    let walked = match kind {
        MediaKind::Files => WxWorkParser::parse_folder_since(media_root, account_id, since)?,
        MediaKind::Videos => {
            WxWorkParser::parse_videos_folder_since(media_root, account_id, since)?
        }
    };
    let changed_known = if since.is_some() {
        db.list_changed_known_files(&root_s)?
    } else {
        Vec::new()
    };
    // 企业微信月份目录为平铺布局，无会话子目录；路径不解析会话 ID。
    let candidates = merge_candidates(
        walked,
        changed_known,
        WXWORK_WINDOWS_SOURCE_TYPE,
        account_id,
        None,
        None,
        None,
    );
    req.emit(ScanEvent::MediaRootCandidates {
        kind,
        path: root_s.clone(),
        count: candidates.len(),
    });

    let complete = ingest_candidates(db, req.device_id, candidates, report)?;
    if complete {
        db.mark_scan_started(
            &root_s,
            WXWORK_WINDOWS_SOURCE_TYPE,
            account_id,
            req.scan_started_ms,
        )?;
    } else {
        req.emit(ScanEvent::MediaRootIncomplete {
            path: root_s.clone(),
        });
    }
    Ok(())
}

/// 探测企业微信账号统计。
pub fn inspect_wxwork_accounts(
    root: &Path,
) -> Result<Vec<crate::accounts::SourceAccountInfo>> {
    let accounts = WxWorkDetector::find_accounts(root)?;
    let root_s = root.to_string_lossy().to_string();
    Ok(accounts
        .into_iter()
        .map(|acc: WxWorkAccount| {
            let files = WxWorkParser::parse_account_files(&acc).unwrap_or_default();
            let videos = WxWorkParser::parse_account_videos(&acc).unwrap_or_default();
            crate::accounts::SourceAccountInfo {
                source_account_id: acc.source_account_id,
                source_dir: acc.files_dir.to_string_lossy().to_string(),
                source_root: root_s.clone(),
                files_count_estimated: files.len(),
                videos_count_estimated: videos.len(),
            }
        })
        .collect())
}

/// 自动探测企业微信根目录。
pub fn detect_wxwork_root() -> Result<PathBuf> {
    WxWorkDetector::detect_root()
}
