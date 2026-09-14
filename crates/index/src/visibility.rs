// ChatVault 对象级可见性：隐藏、恢复与彻底删除，同步写入 journal 事件。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::{JournalEvent, JournalEventType};
use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

/// 彻底删除后供命令层清理远端对象的信息。
#[derive(Debug, Clone)]
pub struct PurgeOutcome {
    pub object_id: String,
    pub hash: String,
    pub size: u64,
    /// 是否曾存在未取消的归档任务（用于提示远端可能有对象）
    pub had_upload_tasks: bool,
}

impl Database {
    /// 对象是否已彻底删除
    pub fn object_is_purged(&self, object_id: &str) -> Result<bool> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM object_purges WHERE object_id = ?1",
                params![object_id],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(count > 0)
    }

    /// 对象是否处于隐藏态
    pub fn object_is_hidden(&self, object_id: &str) -> Result<bool> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM object_hidden WHERE object_id = ?1",
                params![object_id],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(count > 0)
    }

    /// 隐藏内容对象：更新可见性、移除 FTS，并追加 ObjectHidden 事件。
    ///
    /// 幂等：已隐藏或已 purge 时不重复写事件。
    pub fn hide_object(&mut self, device_id: &str, object_id: &str) -> Result<()> {
        self.atomic(|db| {
            ensure_object_exists(&db.conn, object_id)?;
            if db.object_is_purged(object_id)? {
                return Err(ChatVaultError::Internal(
                    "对象已彻底删除，无法隐藏".into(),
                ));
            }
            if db.object_is_hidden(object_id)? {
                return Ok(());
            }
            let now = Utc::now().to_rfc3339();
            let (epoch, seq, logical_clock, event_id) = next_event_identity(&db.conn, device_id)?;
            db.conn
                .execute(
                    "INSERT INTO object_hidden(object_id, hidden_at, event_id, logical_clock, device_id)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(object_id) DO UPDATE SET
                       hidden_at=excluded.hidden_at,
                       event_id=excluded.event_id,
                       logical_clock=excluded.logical_clock,
                       device_id=excluded.device_id",
                    params![object_id, now, event_id, logical_clock, device_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            remove_object_fts(&db.conn, object_id)?;
            db.insert_journal_event_if_absent(&JournalEvent {
                event_id,
                device_id: device_id.to_string(),
                epoch,
                seq,
                logical_clock,
                schema_version: 1,
                event_type: JournalEventType::ObjectHidden,
                payload: serde_json::json!({ "object_id": object_id }),
                created_at: Utc::now(),
            })?;
            Ok(())
        })
    }

    /// 恢复已隐藏对象：清除隐藏标记、重建 FTS，并追加 ObjectRestored。
    ///
    /// 幂等：未隐藏时短路；不重建 local_files 关联。
    pub fn restore_object(&mut self, device_id: &str, object_id: &str) -> Result<()> {
        self.atomic(|db| {
            ensure_object_exists(&db.conn, object_id)?;
            if db.object_is_purged(object_id)? {
                return Err(ChatVaultError::Internal("对象已彻底删除，无法恢复".into()));
            }
            if !db.object_is_hidden(object_id)? {
                return Ok(());
            }
            let (epoch, seq, logical_clock, event_id) = next_event_identity(&db.conn, device_id)?;
            db.conn
                .execute(
                    "DELETE FROM object_hidden WHERE object_id = ?1",
                    params![object_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            rebuild_object_fts(&db.conn, object_id)?;
            db.insert_journal_event_if_absent(&JournalEvent {
                event_id,
                device_id: device_id.to_string(),
                epoch,
                seq,
                logical_clock,
                schema_version: 1,
                event_type: JournalEventType::ObjectRestored,
                payload: serde_json::json!({ "object_id": object_id }),
                created_at: Utc::now(),
            })?;
            Ok(())
        })
    }

    /// 彻底删除已隐藏对象：取消任务、删除本机来源映射与记录，写 purge 标记与事件。
    ///
    /// 不删除微信/采集目录原文件；远端对象由调用方按返回的 hash 清理。
    pub fn purge_object(&mut self, device_id: &str, object_id: &str) -> Result<PurgeOutcome> {
        self.atomic(|db| {
            ensure_object_exists(&db.conn, object_id)?;
            if db.object_is_purged(object_id)? {
                return Err(ChatVaultError::Internal("对象已彻底删除".into()));
            }
            if !db.object_is_hidden(object_id)? {
                return Err(ChatVaultError::Internal("请先隐藏后再彻底删除".into()));
            }
            let (hash, size): (String, i64) = db
                .conn
                .query_row(
                    "SELECT hash, size FROM file_objects WHERE object_id = ?1",
                    params![object_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            let had_upload_tasks: bool = db
                .conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM upload_tasks WHERE object_id = ?1 AND status <> 'cancelled')",
                    params![object_id],
                    |r| r.get(0),
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            db.conn
                .execute(
                    "UPDATE upload_tasks SET status='cancelled', updated_at=?2
                     WHERE object_id = ?1 AND status <> 'cancelled'",
                    params![object_id, Utc::now().to_rfc3339()],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            remove_object_fts(&db.conn, object_id)?;
            db.conn
                .execute(
                    "DELETE FROM local_files WHERE record_id IN
                     (SELECT record_id FROM file_records WHERE object_id = ?1)",
                    params![object_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            db.conn
                .execute(
                    "DELETE FROM file_records WHERE object_id = ?1",
                    params![object_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            db.conn
                .execute(
                    "DELETE FROM object_hidden WHERE object_id = ?1",
                    params![object_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            let now = Utc::now().to_rfc3339();
            let (epoch, seq, logical_clock, event_id) = next_event_identity(&db.conn, device_id)?;
            db.conn
                .execute(
                    "INSERT INTO object_purges(object_id, event_id, logical_clock, device_id, purged_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(object_id) DO UPDATE SET
                       event_id=excluded.event_id,
                       logical_clock=excluded.logical_clock,
                       device_id=excluded.device_id,
                       purged_at=excluded.purged_at",
                    params![object_id, event_id, logical_clock, device_id, now],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            db.insert_journal_event_if_absent(&JournalEvent {
                event_id,
                device_id: device_id.to_string(),
                epoch,
                seq,
                logical_clock,
                schema_version: 1,
                event_type: JournalEventType::ObjectPurged,
                payload: serde_json::json!({ "object_id": object_id }),
                created_at: Utc::now(),
            })?;

            Ok(PurgeOutcome {
                object_id: object_id.to_string(),
                hash,
                size: size.max(0) as u64,
                had_upload_tasks,
            })
        })
    }

    /// 应用远端 ObjectHidden
    pub fn apply_object_hidden_event(&mut self, ev: &JournalEvent) -> Result<()> {
        let object_id = require_object_id(ev)?;
        if self.object_is_purged(&object_id)? {
            return Ok(());
        }
        self.conn
            .execute(
                "INSERT INTO object_hidden(object_id, hidden_at, event_id, logical_clock, device_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(object_id) DO UPDATE SET
                   hidden_at=excluded.hidden_at,
                   event_id=excluded.event_id,
                   logical_clock=excluded.logical_clock,
                   device_id=excluded.device_id
                 WHERE excluded.logical_clock > object_hidden.logical_clock
                    OR (excluded.logical_clock = object_hidden.logical_clock
                        AND (excluded.device_id > object_hidden.device_id
                             OR (excluded.device_id = object_hidden.device_id
                                 AND excluded.event_id >= object_hidden.event_id)))",
                params![
                    object_id,
                    ev.created_at.to_rfc3339(),
                    ev.event_id,
                    ev.logical_clock as i64,
                    ev.device_id
                ],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        remove_object_fts(&self.conn, &object_id)?;
        Ok(())
    }

    /// 应用远端 ObjectRestored
    pub fn apply_object_restored_event(&mut self, ev: &JournalEvent) -> Result<()> {
        let object_id = require_object_id(ev)?;
        if self.object_is_purged(&object_id)? {
            return Ok(());
        }
        // 仅当 restore 时钟不早于当前隐藏标记时清除，避免旧 restore 覆盖新 hide。
        let cleared = self
            .conn
            .execute(
                "DELETE FROM object_hidden
                 WHERE object_id = ?1
                   AND (logical_clock < ?2
                        OR (logical_clock = ?2
                            AND (device_id < ?3
                                 OR (device_id = ?3 AND event_id <= ?4))))",
                params![
                    object_id,
                    ev.logical_clock as i64,
                    ev.device_id,
                    ev.event_id
                ],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        if cleared > 0 {
            rebuild_object_fts(&self.conn, &object_id)?;
        }
        Ok(())
    }

    /// 应用远端 ObjectPurged：清除当前全部来源并写 purge 标记。
    pub fn apply_object_purged_event(&mut self, ev: &JournalEvent) -> Result<()> {
        let object_id = require_object_id(ev)?;
        let exists: bool = self
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM file_objects WHERE object_id = ?1)",
                params![object_id],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        if !exists {
            // 仍写入 purge 标记，压制随后到达的旧 hide/restore。
            self.conn
                .execute(
                    "INSERT INTO object_purges(object_id, event_id, logical_clock, device_id, purged_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(object_id) DO UPDATE SET
                       event_id=excluded.event_id,
                       logical_clock=MAX(object_purges.logical_clock, excluded.logical_clock),
                       device_id=excluded.device_id,
                       purged_at=excluded.purged_at",
                    params![
                        object_id,
                        ev.event_id,
                        ev.logical_clock as i64,
                        ev.device_id,
                        ev.created_at.to_rfc3339()
                    ],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            return Ok(());
        }
        self.conn
            .execute(
                "UPDATE upload_tasks SET status='cancelled', updated_at=?2
                 WHERE object_id = ?1 AND status <> 'cancelled'",
                params![object_id, ev.created_at.to_rfc3339()],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        remove_object_fts(&self.conn, &object_id)?;
        self.conn
            .execute(
                "DELETE FROM local_files WHERE record_id IN
                 (SELECT record_id FROM file_records WHERE object_id = ?1)",
                params![object_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        self.conn
            .execute(
                "DELETE FROM file_records WHERE object_id = ?1",
                params![object_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        self.conn
            .execute(
                "DELETE FROM object_hidden WHERE object_id = ?1",
                params![object_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        self.conn
            .execute(
                "INSERT INTO object_purges(object_id, event_id, logical_clock, device_id, purged_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(object_id) DO UPDATE SET
                   event_id=excluded.event_id,
                   logical_clock=MAX(object_purges.logical_clock, excluded.logical_clock),
                   device_id=excluded.device_id,
                   purged_at=excluded.purged_at",
                params![
                    object_id,
                    ev.event_id,
                    ev.logical_clock as i64,
                    ev.device_id,
                    ev.created_at.to_rfc3339()
                ],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }
}

/// 确认对象存在
fn ensure_object_exists(conn: &rusqlite::Connection, object_id: &str) -> Result<()> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM file_objects WHERE object_id = ?1)",
            params![object_id],
            |r| r.get(0),
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    if !exists {
        return Err(ChatVaultError::Internal(format!(
            "内容对象不存在: {object_id}"
        )));
    }
    Ok(())
}

/// 读取事件 payload 中的 object_id
fn require_object_id(ev: &JournalEvent) -> Result<String> {
    ev.payload
        .get("object_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| ChatVaultError::Internal("对象可见性事件缺少 object_id".into()))
}

/// 计算本机下一条 journal 身份（epoch/seq/时钟/事件 ID）
fn next_event_identity(
    conn: &rusqlite::Connection,
    device_id: &str,
) -> Result<(u64, u64, u64, String)> {
    let epoch: i64 = conn
        .query_row(
            "SELECT CAST(COALESCE((SELECT value FROM app_settings WHERE key=?1), '1') AS INTEGER)",
            params![format!("epoch_{device_id}")],
            |r| r.get(0),
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0)+1 FROM journal_events WHERE device_id=?1 AND epoch=?2",
            params![device_id, epoch],
            |r| r.get(0),
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    let logical_clock: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(logical_clock), 0)+1 FROM journal_events",
            [],
            |r| r.get(0),
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    Ok((
        epoch.max(1) as u64,
        seq.max(1) as u64,
        logical_clock.max(1) as u64,
        Uuid::new_v4().to_string(),
    ))
}

/// 删除对象下全部来源的 FTS 行
fn remove_object_fts(conn: &rusqlite::Connection, object_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM file_search_fts WHERE record_id IN
         (SELECT record_id FROM file_records WHERE object_id = ?1)",
        params![object_id],
    )
    .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    Ok(())
}

/// 为对象下未 tombstone 的来源重建 FTS
fn rebuild_object_fts(conn: &rusqlite::Connection, object_id: &str) -> Result<()> {
    remove_object_fts(conn, object_id)?;
    conn.execute(
        "INSERT INTO file_search_fts (record_id, original_name)
         SELECT r.record_id, r.original_name FROM file_records r
         WHERE r.object_id = ?1
           AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
           AND NOT EXISTS(SELECT 1 FROM file_search_fts f WHERE f.record_id = r.record_id)",
        params![object_id],
    )
    .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    Ok(())
}
