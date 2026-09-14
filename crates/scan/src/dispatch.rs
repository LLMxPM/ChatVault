// ChatVault 按采集源配置类型分发扫描。
use crate::events::ScanEvent;
use crate::report::{ScanReport, ScanRequest};
use chatvault_core::error::Result;
use chatvault_core::models::{
    CollectSource, GENERIC_FOLDER_SOURCE_TYPE, WECHAT_WINDOWS_4_SOURCE_TYPE,
    WXWORK_WINDOWS_SOURCE_TYPE,
};
use chatvault_index::Database;
use std::path::PathBuf;

/// 扫描一组已配置采集源；来源边界检查取消请求。
pub fn scan_collect_sources(
    db: &mut Database,
    sources: &[CollectSource],
    req: &ScanRequest<'_>,
) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    for source in sources {
        if req.cancelled() {
            return Ok(report);
        }
        req.emit(ScanEvent::SourceStart {
            source_type: source.source_type.clone(),
            path: source.path.clone(),
        });
        let root = PathBuf::from(&source.path);
        let partial = match source.source_type.as_str() {
            WECHAT_WINDOWS_4_SOURCE_TYPE => {
                crate::wechat::scan_wechat_source(db, &root, source.enable_videos, req)?
            }
            WXWORK_WINDOWS_SOURCE_TYPE => {
                crate::wxwork::scan_wxwork_source(db, &root, source.enable_videos, req)?
            }
            GENERIC_FOLDER_SOURCE_TYPE => crate::generic::scan_generic_source(db, &root, req)?,
            other => {
                req.emit(ScanEvent::UnknownSourceType {
                    source_type: other.to_string(),
                    path: source.path.clone(),
                });
                continue;
            }
        };
        report.merge(partial);
        req.notify_source_done(&source.path, report);
    }
    Ok(report)
}
