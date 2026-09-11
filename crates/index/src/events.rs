// ChatVault 索引子模块：封装持久化操作与事务边界。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};

use rusqlite::params;

impl Database {
    /// 应用 FileRecordAdded 事件（幂等：object_id/record_id 已存在则跳过）
    pub fn apply_file_record_added_event(
        &mut self,
        ev: &chatvault_core::models::JournalEvent,
    ) -> Result<()> {
        let p = &ev.payload;
        let object_id = p
            .get("object_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let record_id = p
            .get("record_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if object_id.is_empty() || record_id.is_empty() {
            return Err(ChatVaultError::Internal(
                "FileRecordAdded payload 缺少 object_id/record_id".into(),
            ));
        }

        let hash = p
            .get("hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let size = p.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as i64;
        let mime = p
            .get("mime")
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream")
            .to_string();
        let extension = p
            .get("extension")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let created_at = p
            .get("object_created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tx = self
            .conn
            .savepoint()
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        tx.execute(
            "INSERT OR IGNORE INTO file_objects (object_id, hash, size, mime, extension, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![object_id, hash, size, mime, extension, created_at],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let source = p
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("sync")
            .to_string();
        let account_id = p
            .get("account_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let conversation_id = p
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let original_name = p
            .get("original_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let file_time = p
            .get("file_time")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let time_source = p
            .get("time_source")
            .and_then(|v| v.as_str())
            .unwrap_or("mtime")
            .to_string();
        let discovered_at = p
            .get("discovered_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let device_id = p
            .get("device_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&ev.device_id)
            .to_string();

        tx.execute(
            "INSERT OR IGNORE INTO file_records
             (record_id, object_id, source, account_id, conversation_id, original_name, file_time, time_source, discovered_at, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record_id,
                object_id,
                source,
                account_id,
                conversation_id,
                original_name,
                file_time,
                time_source,
                discovered_at,
                device_id
            ],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        // FTS
        tx.execute(
            "INSERT INTO file_search_fts (record_id, original_name)
             SELECT ?1, ?2 WHERE NOT EXISTS(SELECT 1 FROM file_search_fts WHERE record_id=?1)
             AND NOT EXISTS(SELECT 1 FROM record_tombstones WHERE record_id=?1)",
            params![record_id, original_name],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        tx.commit()
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 应用 FileRecordDeleted（软删除：移除 FTS，保留记录供恢复）
    pub fn apply_file_record_deleted_event(
        &mut self,
        ev: &chatvault_core::models::JournalEvent,
    ) -> Result<()> {
        let record_id = ev
            .payload
            .get("record_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if record_id.is_empty() {
            return Err(ChatVaultError::Internal("删除事件缺少 record_id".into()));
        }
        self.conn
            .execute(
                "INSERT INTO record_tombstones(record_id,event_id) VALUES (?1,?2)
             ON CONFLICT(record_id) DO UPDATE SET event_id=MIN(event_id,excluded.event_id)",
                params![record_id, ev.event_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        self.conn
            .execute(
                "DELETE FROM file_search_fts WHERE record_id = ?1",
                params![record_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }
}
