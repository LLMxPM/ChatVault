//! # ChatVault Index 库
//!
//! 提供本地 SQLite 数据库管理、Schema 初始化、增量去重入库与 FTS5 中文检索。

pub mod db;
pub mod query;
pub mod schema;

pub use db::{Database, DatabaseStats, IngestResult};
pub use query::{SearchFilter, SearchResultItem, SearchService};
pub use schema::initialize_schema;
