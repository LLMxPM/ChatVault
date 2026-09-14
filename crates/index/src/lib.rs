//! # ChatVault Index 库
//!
//! 提供本地 SQLite 数据库管理、Schema 初始化、增量去重入库与 FTS5 中文检索。

pub mod db;
pub mod query;
pub mod schema;

pub use db::{Database, DatabaseStats, IngestResult, UploadTaskRow};
pub use query::{
    ObjectLocation, ObjectSearchItem, ObjectSearchPage, ObjectSort, ObjectSourceItem, SearchFilter,
    SearchResultItem, SearchService, TimeField,
};
pub use schema::initialize_schema;

mod atomic;
mod cache_gc;
pub mod cache_policy;
pub mod category;
mod events;
mod identity;
mod ingest;
mod journal;
pub mod scan_state;
mod settings;
mod source_mappings;
mod task_runs;
mod task_status;
mod tasks;
pub mod upload_queue;

pub use identity::VaultResetReport;
pub use scan_state::{
    chrono_ms, system_time_from_ms, system_time_to_ms, utc_from_ms, KnownLocalFile,
};
pub use source_mappings::{SourceAccountRow, SourceConversationRow};
pub use task_runs::{
    item_from_row, stage_from_row, task_run_from_row, NewTaskRunItem, TaskRunItemRow, TaskRunRow,
    TaskRunStageRow,
};
pub use upload_queue::PendingUpload;
