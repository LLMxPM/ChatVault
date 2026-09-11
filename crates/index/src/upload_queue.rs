// ChatVault 上传来源与队列查询：旧任务补建稳定副本，失败任务指数退避。
use crate::Database;
use chatvault_core::error::{ChatVaultError, Result};
use rusqlite::params;
use std::path::PathBuf;

/// 上传任务的最小执行信息。
pub struct PendingUpload {
    pub task_id: String,
    pub hash: String,
}

impl Database {
    /// 返回所有当前可执行任务；按更新时间排序以免失败任务阻塞新文件。
    pub fn pending_uploads(&self) -> Result<Vec<PendingUpload>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.task_id,o.hash FROM upload_tasks t JOIN file_objects o ON t.object_id=o.object_id
             WHERE t.status='queued' OR (t.status='retryable_failed' AND
             (julianday('now')-julianday(t.updated_at))*86400 >= MIN(3600,30*(1 << MIN(t.retry_count-1,7))))
             ORDER BY t.updated_at,t.task_id").map_err(db_error)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PendingUpload {
                    task_id: r.get(0)?,
                    hash: r.get(1)?,
                })
            })
            .map_err(db_error)?;
        rows.map(|r| r.map_err(db_error)).collect()
    }

    /// 执行前再次确认暂停状态。
    pub fn task_is_runnable(&self, id: &str) -> Result<bool> {
        self.conn.query_row("SELECT EXISTS(SELECT 1 FROM upload_tasks WHERE task_id=?1 AND status IN ('queued','retryable_failed'))", [id], |r|r.get(0)).map_err(db_error)
    }

    /// 优先使用历史版本副本；旧数据库没有副本时，仅在源内容匹配时安全补建。
    pub fn upload_source(&mut self, id: &str) -> Result<PathBuf> {
        let (record, hash, original, cache): (String,String,Option<String>,Option<String>) = self.conn.query_row(
            "SELECT t.record_id,o.hash,l.original_path,l.cache_path FROM upload_tasks t JOIN file_objects o ON t.object_id=o.object_id
             LEFT JOIN local_files l ON t.record_id=l.record_id WHERE t.task_id=?1", [id],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(db_error)?;
        if let Some(path) = cache.map(PathBuf::from).filter(|p| p.exists()) {
            chatvault_metadata::verify_file_hash(&path, &hash)?;
            return Ok(path);
        }
        let source = original
            .map(PathBuf::from)
            .ok_or_else(|| ChatVaultError::FileNotFound {
                path: format!("任务 {id} 的历史版本来源已丢失"),
            })?;
        let (cache, actual, _) =
            chatvault_metadata::staging::stage_file(&source, &self.staging_dir)?;
        if actual.hex_hash != hash {
            return Err(ChatVaultError::HashMismatch {
                expected: hash,
                actual: actual.hex_hash,
            });
        }
        self.conn
            .execute(
                "UPDATE local_files SET cache_path=?1 WHERE record_id=?2",
                params![cache.to_string_lossy(), record],
            )
            .map_err(db_error)?;
        Ok(cache)
    }
}

/// 转换 SQLite 错误。
fn db_error(e: rusqlite::Error) -> ChatVaultError {
    ChatVaultError::Database(e.to_string())
}
