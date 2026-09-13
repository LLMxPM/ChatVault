// ChatVault 索引子模块：封装持久化操作与事务边界。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};
use chrono::Utc;
use rusqlite::params;

impl Database {
    /// 列出上传任务（可按状态过滤）
    pub fn list_upload_tasks(
        &self,
        status_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<UploadTaskRow>> {
        let sql = "SELECT t.task_id,t.status,t.retry_count,t.error_message,t.updated_at,
            r.original_name,r.record_id,o.hash,o.size,l.original_path
            FROM upload_tasks t JOIN file_records r ON t.record_id=r.record_id
            JOIN file_objects o ON t.object_id=o.object_id LEFT JOIN local_files l ON t.record_id=l.record_id
            WHERE (?1 IS NULL OR t.status=?1) ORDER BY t.updated_at DESC LIMIT ?2";
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![status_filter, limit.min(500) as i64], |r| {
                Ok(UploadTaskRow {
                    task_id: r.get(0)?,
                    status: r.get(1)?,
                    retry_count: r.get::<_, i64>(2)? as u32,
                    error_message: r.get(3)?,
                    updated_at: r.get(4)?,
                    original_name: r.get(5)?,
                    record_id: r.get(6)?,
                    hash: r.get(7)?,
                    size: r.get::<_, i64>(8)? as u64,
                    original_path: r.get(9)?,
                })
            })
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row.map_err(|e| ChatVaultError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    /// 将任务重新入队（从 failed/missing/paused 回到 queued）
    pub fn requeue_task(&mut self, task_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE upload_tasks SET status = 'queued', error_message = NULL, updated_at = ?1 WHERE task_id = ?2",
                params![now, task_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 暂停任务
    pub fn pause_task(&mut self, task_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE upload_tasks SET status = 'paused', updated_at = ?1 WHERE task_id = ?2",
                params![now, task_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }
}
