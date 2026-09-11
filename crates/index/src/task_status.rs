// ChatVault 索引子模块：封装持久化操作与事务边界。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};
use chrono::Utc;
use rusqlite::params;

impl Database {
    /// 更新上传任务状态
    ///
    /// 职责: 将指定任务标记为成功、失败或缺失，并记录错误信息
    /// 输入:
    ///   - `task_id`: 任务标识
    ///   - `status`: 目标状态，如 `backed_up` / `retryable_failed` / `missing`
    ///   - `error_message`: 失败原因（可选）
    pub fn update_task_status(
        &mut self,
        task_id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        if status == "backed_up" {
            self.conn
                .execute(
                    "UPDATE upload_tasks SET status = ?1, error_message = NULL, updated_at = ?2 WHERE task_id = ?3",
                    params![status, now, task_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        } else {
            self.conn
                .execute(
                    "UPDATE upload_tasks SET status = ?1, error_message = ?2, retry_count = retry_count + 1, updated_at = ?3 WHERE task_id = ?4",
                    params![status, error_message.unwrap_or(""), now, task_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        }
        Ok(())
    }
}
