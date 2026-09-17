// ChatVault 上传队列：查询、重新入队、暂停与缺失任务删除。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};
use chrono::Utc;
use rusqlite::params;

impl Database {
    /// 列出上传任务（可按状态过滤）
    ///
    /// `status_filter`：
    /// - `None` 或空串：全部状态
    /// - `pending`：待处理（queued / retryable_failed / missing / paused）
    /// - 其他：精确匹配单个状态
    pub fn list_upload_tasks(
        &self,
        status_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<UploadTaskRow>> {
        let sql = "SELECT t.task_id,t.status,t.retry_count,t.error_message,t.updated_at,
            r.original_name,r.record_id,o.hash,o.size,l.original_path
            FROM upload_tasks t JOIN file_records r ON t.record_id=r.record_id
            JOIN file_objects o ON t.object_id=o.object_id LEFT JOIN local_files l ON t.record_id=l.record_id
            WHERE (
                ?1 IS NULL OR ?1 = ''
                OR (?1 = 'pending' AND t.status IN ('queued','retryable_failed','missing','paused'))
                OR t.status = ?1
            ) ORDER BY t.updated_at DESC LIMIT ?2";
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
        let changed = self.conn
            .execute(
                "UPDATE upload_tasks SET status = 'queued', error_message = NULL, updated_at = ?1 WHERE task_id = ?2",
                params![now, task_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        if changed == 0 {
            return Err(ChatVaultError::Internal(
                "上传任务已不存在，请刷新后重试".into(),
            ));
        }
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

    /// 仅删除当前仍为 missing 的队列项；保留文件索引与历史明细，清除历史的重试关联。
    /// 返回是否实际删除；状态已变化或任务不存在时返回 false。
    pub fn delete_missing_upload_task(&mut self, task_id: &str) -> Result<bool> {
        self.atomic(|db| {
            let changed = db
                .conn
                .execute(
                    "DELETE FROM upload_tasks WHERE task_id = ?1 AND status = 'missing'",
                    [task_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            if changed == 0 {
                return Ok(false);
            }
            db.conn
                .execute(
                    "UPDATE task_run_items SET task_id = NULL WHERE task_id = ?1",
                    [task_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            Ok(true)
        })
    }
}
