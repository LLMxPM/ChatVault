//! # ChatVault 扫描编排库
//!
//! 统一桌面端与 CLI 的采集源扫描：按 `source_type` 分发适配器、合并已知
//! 文件变更、稳定性检测入库并维护扫描检查点。新增来源类型只需在本 crate
//! 增加分支与适配器，两端调用方共用同一套编排。

pub mod accounts;
mod dispatch;
mod events;
mod generic;
mod ingest;
mod report;
mod wechat;
mod wxwork;

pub use accounts::{is_selected_account, AccountMediaRoots, AccountTarget, SourceAccountInfo};
pub use dispatch::scan_collect_sources;
pub use events::{MediaKind, ScanEvent};
pub use generic::scan_generic_source;
pub use ingest::{ingest_candidates, merge_candidates, resolve_since};
pub use report::{ScanReport, ScanRequest};
pub use wechat::{detect_wechat_root, inspect_wechat_accounts, scan_wechat_source};
pub use wxwork::{detect_wxwork_root, inspect_wxwork_accounts, scan_wxwork_source};
