// ChatVault 微信 4.x 采集源扫描编排。
use crate::accounts::AccountMediaRoots;
use crate::events::{MediaKind, ScanEvent};
use crate::ingest::{ingest_candidates, merge_candidates, resolve_since};
use crate::report::{account_selected, ScanReport, ScanRequest};
use adapter_wechat_windows::{WeChat4Detector, WeChat4Parser};
use chatvault_core::error::Result;
use chatvault_core::models::WECHAT_WINDOWS_4_SOURCE_TYPE;
use chatvault_index::Database;
use std::path::{Path, PathBuf};

/// 扫描一个微信 4.x 根目录下的全部或勾选账号。
pub fn scan_wechat_source(
    db: &mut Database,
    root: &Path,
    enable_videos: bool,
    req: &ScanRequest<'_>,
) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    if !root.is_dir() {
        req.emit(ScanEvent::SourceMissing {
            path: root.display().to_string(),
            label: "微信 4.x 采集目录不存在，跳过扫描",
        });
        return Ok(report);
    }
    let accounts = WeChat4Detector::find_accounts(root)?;
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
            WECHAT_WINDOWS_4_SOURCE_TYPE,
            Some(&acc.source_account_id),
            MediaKind::Files,
            true,
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
            WECHAT_WINDOWS_4_SOURCE_TYPE,
            Some(&acc.source_account_id),
            MediaKind::Videos,
            false,
            req,
            &mut report,
        )?;
    }
    Ok(report)
}

/// 扫描微信单个媒体根（msg/file 或 msg/video）。
#[allow(clippy::too_many_arguments)]
fn scan_media_root(
    db: &mut Database,
    media_root: &Path,
    source_type: &str,
    account_id: Option<&str>,
    kind: MediaKind,
    resolve_conversation: bool,
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
        MediaKind::Files => WeChat4Parser::parse_folder_since(media_root, account_id, since)?,
        MediaKind::Videos => {
            WeChat4Parser::parse_videos_folder_since(media_root, account_id, since)?
        }
    };
    let changed_known = if since.is_some() {
        db.list_changed_known_files(&root_s)?
    } else {
        Vec::new()
    };
    let conversation_fn: &dyn Fn(&Path, &Path) -> Option<String> =
        &|root, file| WeChat4Parser::conversation_id_for_path(root, file);
    let candidates = merge_candidates(
        walked,
        changed_known,
        source_type,
        account_id,
        None,
        if resolve_conversation {
            Some(media_root)
        } else {
            None
        },
        if resolve_conversation {
            Some(conversation_fn)
        } else {
            None
        },
    );
    req.emit(ScanEvent::MediaRootCandidates {
        kind,
        path: root_s.clone(),
        count: candidates.len(),
    });

    let complete = ingest_candidates(db, req.device_id, candidates, report)?;
    if complete {
        db.mark_scan_started(&root_s, source_type, account_id, req.scan_started_ms)?;
    } else {
        req.emit(ScanEvent::MediaRootIncomplete {
            path: root_s.clone(),
        });
    }
    Ok(())
}

/// 探测微信账号统计（用于任务页账号列表）。
pub fn inspect_wechat_accounts(root: &Path) -> Result<Vec<crate::accounts::SourceAccountInfo>> {
    let accounts = WeChat4Detector::find_accounts(root)?;
    let root_s = root.to_string_lossy().to_string();
    Ok(accounts
        .into_iter()
        .map(|acc| {
            let files = WeChat4Parser::parse_account_files(&acc).unwrap_or_default();
            let videos = WeChat4Parser::parse_account_videos(&acc).unwrap_or_default();
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

/// 将微信账号结构转为共享媒体根描述（供探测统计复用）。
#[allow(dead_code)]
pub(crate) fn wechat_media_roots(acc: &adapter_wechat_windows::WeChatAccount) -> AccountMediaRoots {
    AccountMediaRoots {
        source_account_id: acc.source_account_id.clone(),
        root_dir: acc.root_dir.clone(),
        files_dir: acc.files_dir.clone(),
        video_dir: acc.video_dir.clone(),
    }
}

/// 自动探测微信 4.x 根目录。
pub fn detect_wechat_root() -> Result<PathBuf> {
    WeChat4Detector::detect_root()
}
