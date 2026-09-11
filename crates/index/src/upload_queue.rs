// ChatVault 上传来源与队列查询：校验受控副本，失败任务指数退避。
use crate::Database;
use chatvault_core::error::{ChatVaultError, Result};
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

    /// 有副本则严格使用副本；未创建副本时读取原路径，两者都必须匹配入库哈希。
    pub fn upload_source(&self, id: &str) -> Result<PathBuf> {
        let (hash, cache, original): (String, Option<String>, String) = self.conn.query_row(
            "SELECT o.hash,l.cache_path,l.original_path FROM upload_tasks t JOIN file_objects o ON t.object_id=o.object_id
             LEFT JOIN local_files l ON t.record_id=l.record_id WHERE t.task_id=?1", [id],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(db_error)?;
        let selected = cache.unwrap_or(original);
        let path = Some(PathBuf::from(&selected))
            .filter(|p| p.is_file())
            .ok_or_else(|| ChatVaultError::FileNotFound {
                path: format!("任务 {id} 的上传来源已丢失：{selected}"),
            })?;
        chatvault_metadata::verify_file_hash(&path, &hash)?;
        Ok(path)
    }
}

/// 转换 SQLite 错误。
fn db_error(e: rusqlite::Error) -> ChatVaultError {
    ChatVaultError::Database(e.to_string())
}
