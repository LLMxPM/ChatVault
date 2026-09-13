//! # ChatVault Sync 库
//!
//! 实现文档 §5/§6 的元数据同步：本机事件生成、不可变日志分片与 commit 标记发布、
//! 远端事件拉取合并、游标推进与空索引恢复。

pub mod apply;
pub mod archive;
pub mod progress;
pub mod publish;
mod validation;

pub use apply::{pull_and_apply, restore_from_remote};
pub use archive::{archive_pending, archive_pending_with_progress, ArchiveReport};
pub use progress::{ArchiveProgressSink, NoopProgressSink};
pub use publish::{publish_pending_events, JournalPublisher};
