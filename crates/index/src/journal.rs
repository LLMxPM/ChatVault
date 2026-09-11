// ChatVault 索引子模块：封装持久化操作与事务边界。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};
use chrono::Utc;
use rusqlite::params;

impl Database {
    // ========== 同步：事件 / 游标 / 设备 ==========

    /// 插入本机日志事件（若 event_id 不存在）
    pub fn insert_journal_event_if_absent(
        &mut self,
        ev: &chatvault_core::models::JournalEvent,
    ) -> Result<()> {
        // 同一个 event_id 必须始终表示同一事件，不能用幂等插入掩盖内容冲突。
        let existing = self.conn.query_row(
            "SELECT event_id,device_id,epoch,seq,logical_clock,schema_version,event_type,payload,created_at FROM journal_events WHERE event_id=?1",
            [&ev.event_id], row_to_journal_event);
        match existing {
            Ok(old) if serde_json::to_value(&old)? != serde_json::to_value(ev)? => {
                return Err(ChatVaultError::Database("相同事件 ID 对应不同内容".into()));
            }
            Ok(_) => return Ok(()),
            Err(rusqlite::Error::QueryReturnedNoRows) => {}
            Err(e) => return Err(ChatVaultError::Database(e.to_string())),
        }
        self.conn
            .execute(
                "INSERT INTO journal_events
                 (event_id, device_id, epoch, seq, logical_clock, schema_version, event_type, payload, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT(event_id) DO NOTHING",
                params![
                    ev.event_id,
                    ev.device_id,
                    ev.epoch as i64,
                    ev.seq as i64,
                    ev.logical_clock as i64,
                    ev.schema_version as i64,
                    event_type_str(ev.event_type),
                    ev.payload.to_string(),
                    ev.created_at.to_rfc3339()
                ],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 列出指定设备 seq 大于 after_seq 的事件，按 seq 升序
    pub fn list_unpublished_journal_events(
        &self,
        device_id: &str,
        after_seq: u64,
        limit: u64,
    ) -> Result<Vec<chatvault_core::models::JournalEvent>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT event_id, device_id, epoch, seq, logical_clock, schema_version, event_type, payload, created_at
                 FROM journal_events
                 WHERE device_id = ?1 AND seq > ?2
                 ORDER BY seq ASC
                 LIMIT ?3",
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![device_id, after_seq as i64, limit as i64], |r| {
                row_to_journal_event(r)
            })
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for row in rows {
            list.push(row.map_err(|e| ChatVaultError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    /// 列出指定设备 seq 大于 after_seq 的事件，按 seq 升序
    pub fn list_journal_events_epoch(
        &self,
        device_id: &str,
        epoch: u64,
        after_seq: u64,
        limit: u64,
    ) -> Result<Vec<chatvault_core::models::JournalEvent>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT event_id, device_id, epoch, seq, logical_clock, schema_version, event_type, payload, created_at
                 FROM journal_events
                 WHERE device_id = ?1 AND seq > ?2 AND epoch = ?4
                 ORDER BY seq ASC
                 LIMIT ?3",
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(
                params![device_id, after_seq as i64, limit as i64, epoch as i64],
                |r| row_to_journal_event(r),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for row in rows {
            list.push(row.map_err(|e| ChatVaultError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    /// 获取指定设备的下一个本地 seq（max+1）
    pub fn next_journal_seq(&self, device_id: &str) -> Result<u64> {
        let max: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM journal_events WHERE device_id = ?1",
                params![device_id],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok((max as u64) + 1)
    }

    /// 获取指定设备的下一个逻辑时钟
    pub fn next_logical_clock(&self) -> Result<u64> {
        let max: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(logical_clock), 0) FROM journal_events",
                [],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok((max as u64) + 1)
    }

    /// 读取同步游标
    pub fn get_sync_cursor(
        &self,
        device_id: &str,
    ) -> Result<Option<chatvault_core::models::SyncCursor>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT device_id, epoch, last_contiguous_seq FROM sync_cursors WHERE device_id = ?1 ORDER BY epoch DESC LIMIT 1",
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let cur = stmt
            .query_row(params![device_id], |r| {
                Ok(chatvault_core::models::SyncCursor {
                    device_id: r.get(0)?,
                    epoch: r.get::<_, i64>(1)? as u64,
                    last_contiguous_seq: r.get::<_, i64>(2)? as u64,
                })
            })
            .ok();
        Ok(cur)
    }

    /// 写入/更新同步游标
    pub fn upsert_sync_cursor(
        &mut self,
        device_id: &str,
        epoch: u64,
        last_contiguous_seq: u64,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO sync_cursors (device_id, epoch, last_contiguous_seq) VALUES (?1, ?2, ?3)
                 ON CONFLICT(device_id,epoch) DO UPDATE SET last_contiguous_seq = MAX(sync_cursors.last_contiguous_seq,excluded.last_contiguous_seq)",
                params![device_id, epoch as i64, last_contiguous_seq as i64],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 读取指定 epoch 的连续游标，设备重建不会覆盖历史 epoch 的进度。
    pub fn cursor_seq(&self, device: &str, epoch: u64) -> Result<u64> {
        self.conn.query_row("SELECT COALESCE((SELECT last_contiguous_seq FROM sync_cursors WHERE device_id=?1 AND epoch=?2),0)",
            params![device,epoch],|r|r.get(0)).map_err(|e|ChatVaultError::Database(e.to_string()))
    }

    /// 判断事件是否已应用
    pub fn event_already_applied(&self, event_id: &str) -> Result<bool> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM applied_events WHERE event_id = ?1",
                params![event_id],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(count > 0)
    }

    /// 标记事件已应用
    pub fn mark_event_applied(&mut self, event_id: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO applied_events (event_id, applied_at) VALUES (?1, ?2)",
                params![event_id, Utc::now().to_rfc3339()],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 读取已知设备
    pub fn get_known_device(
        &self,
        device_id: &str,
    ) -> Result<Option<chatvault_core::models::DeviceInfo>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT device_id, display_name, epoch, last_seq, updated_at FROM known_devices WHERE device_id = ?1",
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let info = stmt
            .query_row(params![device_id], |r| {
                Ok(chatvault_core::models::DeviceInfo {
                    device_id: r.get(0)?,
                    display_name: r.get(1)?,
                    epoch: r.get::<_, i64>(2)? as u64,
                    last_seq: r.get::<_, i64>(3)? as u64,
                    updated_at: chrono::DateTime::parse_from_rfc3339(&r.get::<_, String>(4)?)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .ok();
        Ok(info)
    }

    /// 写入/更新已知设备
    pub fn upsert_known_device(
        &mut self,
        device: &chatvault_core::models::DeviceInfo,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO known_devices (device_id, display_name, epoch, last_seq, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(device_id) DO UPDATE SET
                   display_name = excluded.display_name,
                   epoch = excluded.epoch,
                   last_seq = excluded.last_seq,
                   updated_at = excluded.updated_at",
                params![
                    device.device_id,
                    device.display_name,
                    device.epoch as i64,
                    device.last_seq as i64,
                    device.updated_at.to_rfc3339()
                ],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 列出全部已知设备
    pub fn list_known_devices(&self) -> Result<Vec<chatvault_core::models::DeviceInfo>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT device_id, display_name, epoch, last_seq, updated_at FROM known_devices ORDER BY device_id",
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(chatvault_core::models::DeviceInfo {
                    device_id: r.get(0)?,
                    display_name: r.get(1)?,
                    epoch: r.get::<_, i64>(2)? as u64,
                    last_seq: r.get::<_, i64>(3)? as u64,
                    updated_at: chrono::DateTime::parse_from_rfc3339(&r.get::<_, String>(4)?)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row.map_err(|e| ChatVaultError::Database(e.to_string()))?);
        }
        Ok(list)
    }
}

fn event_type_str(t: chatvault_core::models::JournalEventType) -> &'static str {
    match t {
        chatvault_core::models::JournalEventType::FileRecordAdded => "file_record_added",
        chatvault_core::models::JournalEventType::FileRecordDeleted => "file_record_deleted",
    }
}

fn row_to_journal_event(
    r: &rusqlite::Row,
) -> rusqlite::Result<chatvault_core::models::JournalEvent> {
    let event_type_str: String = r.get(6)?;
    let payload_str: String = r.get(7)?;
    let created_at: String = r.get(8)?;
    let event_type = match event_type_str.as_str() {
        "file_record_deleted" => chatvault_core::models::JournalEventType::FileRecordDeleted,
        _ => chatvault_core::models::JournalEventType::FileRecordAdded,
    };
    Ok(chatvault_core::models::JournalEvent {
        event_id: r.get(0)?,
        device_id: r.get(1)?,
        epoch: r.get::<_, i64>(2)? as u64,
        seq: r.get::<_, i64>(3)? as u64,
        logical_clock: r.get::<_, i64>(4)? as u64,
        schema_version: r.get::<_, i64>(5)? as u32,
        event_type,
        payload: serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null),
        created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}
