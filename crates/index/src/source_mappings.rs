//! ChatVault 来源实体映射：账号/聊天的原始标识、用户名称、收藏状态与同步版本。

use crate::Database;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::{JournalEvent, JournalEventType};
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

/// 来源账号映射的查询结果。
#[derive(Debug, Clone)]
pub struct SourceAccountRow {
    pub source_type: String,
    pub source_account_id: String,
    pub source_name: Option<String>,
    pub display_name: Option<String>,
    pub effective_name: String,
    pub is_favorite: bool,
    pub record_count: usize,
}

/// 来源聊天映射的查询结果。
#[derive(Debug, Clone)]
pub struct SourceConversationRow {
    pub source_type: String,
    pub source_account_id: String,
    pub source_conversation_id: String,
    pub source_name: Option<String>,
    pub display_name: Option<String>,
    pub effective_name: String,
    pub is_favorite: bool,
    pub record_count: usize,
}

/// 校验来源类型或来源 ID；来源标识不要求路径安全，但不能为空且长度必须受限。
pub(crate) fn validate_source_key(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 512 {
        return Err(ChatVaultError::Internal(format!("{field} 无效")));
    }
    Ok(())
}

/// 返回用户界面使用的名称：自定义名称、原始名称、稳定 ID 依次回退。
pub(crate) fn effective_name(
    display_name: Option<&str>,
    source_name: Option<&str>,
    source_id: &str,
) -> String {
    display_name
        .filter(|name| !name.trim().is_empty())
        .or_else(|| source_name.filter(|name| !name.trim().is_empty()))
        .unwrap_or(source_id)
        .to_string()
}

/// 确保来源账号存在；重复扫描只补齐原始名称，不覆盖用户维护字段。
pub(crate) fn ensure_source_account(
    conn: &Connection,
    source_type: &str,
    source_account_id: &str,
    source_name: Option<&str>,
    now: &str,
) -> Result<()> {
    validate_source_key(source_type, "source_type")?;
    validate_source_key(source_account_id, "source_account_id")?;
    conn.execute(
        "INSERT INTO source_accounts
         (source_type, source_account_id, source_name, display_name, is_favorite,
          created_at, updated_at, updated_logical_clock, updated_device_id, updated_event_id)
         VALUES (?1, ?2, ?3, NULL, 0, ?4, ?4, 0, '', '')
         ON CONFLICT(source_type, source_account_id) DO UPDATE SET
           source_name = COALESCE(source_accounts.source_name, excluded.source_name)",
        params![source_type, source_account_id, source_name, now],
    )
    .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    Ok(())
}

/// 确保来源聊天存在；只有账号 ID 和聊天 ID 同时存在时调用此函数。
pub(crate) fn ensure_source_conversation(
    conn: &Connection,
    source_type: &str,
    source_account_id: &str,
    source_conversation_id: &str,
    source_name: Option<&str>,
    now: &str,
) -> Result<()> {
    validate_source_key(source_type, "source_type")?;
    validate_source_key(source_account_id, "source_account_id")?;
    validate_source_key(source_conversation_id, "source_conversation_id")?;
    conn.execute(
        "INSERT INTO source_conversations
         (source_type, source_account_id, source_conversation_id, source_name, display_name,
          is_favorite, created_at, updated_at, updated_logical_clock, updated_device_id, updated_event_id)
         VALUES (?1, ?2, ?3, ?4, NULL, 0, ?5, ?5, 0, '', '')
         ON CONFLICT(source_type, source_account_id, source_conversation_id) DO UPDATE SET
           source_name = COALESCE(source_conversations.source_name, excluded.source_name)",
        params![
            source_type,
            source_account_id,
            source_conversation_id,
            source_name,
            now
        ],
    )
    .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    Ok(())
}

/// 使用确定性版本序更新账号映射，旧版本或重复事件不会覆盖新状态。
pub(crate) fn upsert_source_account_version(
    conn: &Connection,
    source_type: &str,
    source_account_id: &str,
    display_name: Option<&str>,
    is_favorite: bool,
    created_at: &str,
    updated_at: &str,
    logical_clock: u64,
    device_id: &str,
    event_id: &str,
) -> Result<()> {
    validate_source_key(source_type, "source_type")?;
    validate_source_key(source_account_id, "source_account_id")?;
    conn.execute(
        "INSERT INTO source_accounts
         (source_type, source_account_id, source_name, display_name, is_favorite,
          created_at, updated_at, updated_logical_clock, updated_device_id, updated_event_id)
         VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(source_type, source_account_id) DO UPDATE SET
           display_name = excluded.display_name,
           is_favorite = excluded.is_favorite,
           updated_at = excluded.updated_at,
           updated_logical_clock = excluded.updated_logical_clock,
           updated_device_id = excluded.updated_device_id,
           updated_event_id = excluded.updated_event_id
         WHERE excluded.updated_logical_clock > source_accounts.updated_logical_clock
            OR (excluded.updated_logical_clock = source_accounts.updated_logical_clock
                AND excluded.updated_device_id > source_accounts.updated_device_id)
            OR (excluded.updated_logical_clock = source_accounts.updated_logical_clock
                AND excluded.updated_device_id = source_accounts.updated_device_id
                AND excluded.updated_event_id > source_accounts.updated_event_id)",
        params![
            source_type,
            source_account_id,
            display_name,
            i64::from(is_favorite),
            created_at,
            updated_at,
            logical_clock as i64,
            device_id,
            event_id
        ],
    )
    .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    Ok(())
}

/// 使用确定性版本序更新聊天映射，旧版本或重复事件不会覆盖新状态。
pub(crate) fn upsert_source_conversation_version(
    conn: &Connection,
    source_type: &str,
    source_account_id: &str,
    source_conversation_id: &str,
    display_name: Option<&str>,
    is_favorite: bool,
    created_at: &str,
    updated_at: &str,
    logical_clock: u64,
    device_id: &str,
    event_id: &str,
) -> Result<()> {
    validate_source_key(source_type, "source_type")?;
    validate_source_key(source_account_id, "source_account_id")?;
    validate_source_key(source_conversation_id, "source_conversation_id")?;
    conn.execute(
        "INSERT INTO source_conversations
         (source_type, source_account_id, source_conversation_id, source_name, display_name,
          is_favorite, created_at, updated_at, updated_logical_clock, updated_device_id, updated_event_id)
         VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(source_type, source_account_id, source_conversation_id) DO UPDATE SET
           display_name = excluded.display_name,
           is_favorite = excluded.is_favorite,
           updated_at = excluded.updated_at,
           updated_logical_clock = excluded.updated_logical_clock,
           updated_device_id = excluded.updated_device_id,
           updated_event_id = excluded.updated_event_id
         WHERE excluded.updated_logical_clock > source_conversations.updated_logical_clock
            OR (excluded.updated_logical_clock = source_conversations.updated_logical_clock
                AND excluded.updated_device_id > source_conversations.updated_device_id)
            OR (excluded.updated_logical_clock = source_conversations.updated_logical_clock
                AND excluded.updated_device_id = source_conversations.updated_device_id
                AND excluded.updated_event_id > source_conversations.updated_event_id)",
        params![
            source_type,
            source_account_id,
            source_conversation_id,
            display_name,
            i64::from(is_favorite),
            created_at,
            updated_at,
            logical_clock as i64,
            device_id,
            event_id
        ],
    )
    .map_err(|e| ChatVaultError::Database(e.to_string()))?;
    Ok(())
}

impl Database {
    /// 列出全部来源账号并统计未删除文件记录数量。
    pub fn list_source_accounts(&self) -> Result<Vec<SourceAccountRow>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT a.source_type, a.source_account_id, a.source_name, a.display_name,
                        a.is_favorite, COUNT(r.record_id)
                 FROM source_accounts a
                 LEFT JOIN file_records r
                   ON r.source_type = a.source_type
                  AND r.source_account_id = a.source_account_id
                  AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
                 GROUP BY a.source_type, a.source_account_id, a.source_name, a.display_name, a.is_favorite
                 ORDER BY a.is_favorite DESC, a.source_type, a.source_account_id",
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let source_id: String = row.get(1)?;
                let source_name: Option<String> = row.get(2)?;
                let display_name: Option<String> = row.get(3)?;
                Ok(SourceAccountRow {
                    source_type: row.get(0)?,
                    source_account_id: source_id.clone(),
                    effective_name: effective_name(
                        display_name.as_deref(),
                        source_name.as_deref(),
                        &source_id,
                    ),
                    source_name,
                    display_name,
                    is_favorite: row.get::<_, i64>(4)? != 0,
                    record_count: row.get::<_, i64>(5)? as usize,
                })
            })
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        rows.map(|row| row.map_err(|e| ChatVaultError::Database(e.to_string())))
            .collect()
    }

    /// 按来源类型和账号列出来源聊天并统计未删除文件记录数量。
    pub fn list_source_conversations(
        &self,
        source_type: Option<&str>,
        source_account_id: Option<&str>,
    ) -> Result<Vec<SourceConversationRow>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.source_type, c.source_account_id, c.source_conversation_id,
                        c.source_name, c.display_name, c.is_favorite, COUNT(r.record_id)
                 FROM source_conversations c
                 LEFT JOIN file_records r
                   ON r.source_type = c.source_type
                  AND r.source_account_id = c.source_account_id
                  AND r.source_conversation_id = c.source_conversation_id
                  AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
                 WHERE (?1 IS NULL OR c.source_type = ?1)
                   AND (?2 IS NULL OR c.source_account_id = ?2)
                 GROUP BY c.source_type, c.source_account_id, c.source_conversation_id,
                          c.source_name, c.display_name, c.is_favorite
                 ORDER BY c.is_favorite DESC, c.source_type, c.source_account_id, c.source_conversation_id",
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![source_type, source_account_id], |row| {
                let source_id: String = row.get(2)?;
                let source_name: Option<String> = row.get(3)?;
                let display_name: Option<String> = row.get(4)?;
                Ok(SourceConversationRow {
                    source_type: row.get(0)?,
                    source_account_id: row.get(1)?,
                    source_conversation_id: source_id.clone(),
                    effective_name: effective_name(
                        display_name.as_deref(),
                        source_name.as_deref(),
                        &source_id,
                    ),
                    source_name,
                    display_name,
                    is_favorite: row.get::<_, i64>(5)? != 0,
                    record_count: row.get::<_, i64>(6)? as usize,
                })
            })
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        rows.map(|row| row.map_err(|e| ChatVaultError::Database(e.to_string())))
            .collect()
    }

    /// 在同一保存点中更新账号映射并追加 SourceAccountUpdated 事件。
    pub fn update_source_account(
        &mut self,
        device_id: &str,
        source_type: &str,
        source_account_id: &str,
        display_name: Option<&str>,
        is_favorite: bool,
    ) -> Result<()> {
        let display_name = display_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        self.atomic(|db| {
            let now = Utc::now().to_rfc3339();
            ensure_source_account(&db.conn, source_type, source_account_id, None, &now)?;
            let epoch = db
                .get_setting(&format!("epoch_{device_id}"))?
                .and_then(|value| value.parse().ok())
                .unwrap_or(1u64);
            let seq = db.next_journal_seq(device_id)?;
            let logical_clock = db.next_logical_clock()?;
            let event_id = Uuid::new_v4().to_string();
            upsert_source_account_version(
                &db.conn,
                source_type,
                source_account_id,
                display_name.as_deref(),
                is_favorite,
                &now,
                &now,
                logical_clock,
                device_id,
                &event_id,
            )?;
            db.insert_journal_event_if_absent(&JournalEvent {
                event_id,
                device_id: device_id.to_string(),
                epoch,
                seq,
                logical_clock,
                schema_version: 1,
                event_type: JournalEventType::SourceAccountUpdated,
                payload: serde_json::json!({
                    "source_type": source_type,
                    "source_account_id": source_account_id,
                    "display_name": display_name,
                    "is_favorite": is_favorite,
                }),
                created_at: Utc::now(),
            })?;
            Ok(())
        })
    }

    /// 在同一保存点中更新聊天映射并追加 SourceConversationUpdated 事件。
    pub fn update_source_conversation(
        &mut self,
        device_id: &str,
        source_type: &str,
        source_account_id: &str,
        source_conversation_id: &str,
        display_name: Option<&str>,
        is_favorite: bool,
    ) -> Result<()> {
        let display_name = display_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        self.atomic(|db| {
            let now = Utc::now().to_rfc3339();
            ensure_source_account(&db.conn, source_type, source_account_id, None, &now)?;
            ensure_source_conversation(
                &db.conn,
                source_type,
                source_account_id,
                source_conversation_id,
                None,
                &now,
            )?;
            let epoch = db
                .get_setting(&format!("epoch_{device_id}"))?
                .and_then(|value| value.parse().ok())
                .unwrap_or(1u64);
            let seq = db.next_journal_seq(device_id)?;
            let logical_clock = db.next_logical_clock()?;
            let event_id = Uuid::new_v4().to_string();
            upsert_source_conversation_version(
                &db.conn,
                source_type,
                source_account_id,
                source_conversation_id,
                display_name.as_deref(),
                is_favorite,
                &now,
                &now,
                logical_clock,
                device_id,
                &event_id,
            )?;
            db.insert_journal_event_if_absent(&JournalEvent {
                event_id,
                device_id: device_id.to_string(),
                epoch,
                seq,
                logical_clock,
                schema_version: 1,
                event_type: JournalEventType::SourceConversationUpdated,
                payload: serde_json::json!({
                    "source_type": source_type,
                    "source_account_id": source_account_id,
                    "source_conversation_id": source_conversation_id,
                    "display_name": display_name,
                    "is_favorite": is_favorite,
                }),
                created_at: Utc::now(),
            })?;
            Ok(())
        })
    }

    /// 应用远端账号映射事件，先创建占位映射再按版本序更新。
    pub fn apply_source_account_updated_event(&mut self, ev: &JournalEvent) -> Result<()> {
        let source_type = required_payload_string(&ev.payload, "source_type")?;
        let source_account_id = required_payload_string(&ev.payload, "source_account_id")?;
        let display_name = optional_display_name(&ev.payload)?;
        let is_favorite = required_favorite(&ev.payload)?;
        let now = ev.created_at.to_rfc3339();
        ensure_source_account(&self.conn, &source_type, &source_account_id, None, &now)?;
        upsert_source_account_version(
            &self.conn,
            &source_type,
            &source_account_id,
            display_name.as_deref(),
            is_favorite,
            &now,
            &now,
            ev.logical_clock,
            &ev.device_id,
            &ev.event_id,
        )
    }

    /// 应用远端聊天映射事件，保证对应账号占位映射存在。
    pub fn apply_source_conversation_updated_event(&mut self, ev: &JournalEvent) -> Result<()> {
        let source_type = required_payload_string(&ev.payload, "source_type")?;
        let source_account_id = required_payload_string(&ev.payload, "source_account_id")?;
        let source_conversation_id =
            required_payload_string(&ev.payload, "source_conversation_id")?;
        let display_name = optional_display_name(&ev.payload)?;
        let is_favorite = required_favorite(&ev.payload)?;
        let now = ev.created_at.to_rfc3339();
        ensure_source_account(&self.conn, &source_type, &source_account_id, None, &now)?;
        ensure_source_conversation(
            &self.conn,
            &source_type,
            &source_account_id,
            &source_conversation_id,
            None,
            &now,
        )?;
        upsert_source_conversation_version(
            &self.conn,
            &source_type,
            &source_account_id,
            &source_conversation_id,
            display_name.as_deref(),
            is_favorite,
            &now,
            &now,
            ev.logical_clock,
            &ev.device_id,
            &ev.event_id,
        )
    }
}

/// 从事件 payload 读取必需的非空来源字段。
fn required_payload_string(payload: &serde_json::Value, field: &str) -> Result<String> {
    let value = payload
        .get(field)
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    validate_source_key(value, field)?;
    Ok(value.to_string())
}

/// 读取可为空的自定义名称，并统一空白字符串为 NULL 语义。
fn optional_display_name(payload: &serde_json::Value) -> Result<Option<String>> {
    let Some(value) = payload.get("display_name") else {
        return Err(ChatVaultError::Internal("映射事件缺少 display_name".into()));
    };
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        _ => Err(ChatVaultError::Internal("display_name 类型无效".into())),
    }
}

/// 读取映射事件的收藏状态。
fn required_favorite(payload: &serde_json::Value) -> Result<bool> {
    payload
        .get("is_favorite")
        .and_then(|value| value.as_bool())
        .ok_or_else(|| ChatVaultError::Internal("映射事件缺少 is_favorite".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn account_update_is_atomic_and_clear_name_uses_null() {
        let mut db = Database::open_in_memory().unwrap();
        db.update_source_account(
            "device-a",
            "wechat-windows-4",
            "wxid_abc",
            Some("张三的微信"),
            true,
        )
        .unwrap();
        let rows = db.list_source_accounts().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].effective_name, "张三的微信");
        assert!(rows[0].is_favorite);

        db.update_source_account(
            "device-a",
            "wechat-windows-4",
            "wxid_abc",
            Some("  "),
            false,
        )
        .unwrap();
        let rows = db.list_source_accounts().unwrap();
        assert_eq!(rows[0].display_name, None);
        assert_eq!(rows[0].effective_name, "wxid_abc");
        assert!(!rows[0].is_favorite);
        assert_eq!(
            db.connection()
                .query_row(
                    "SELECT COUNT(*) FROM journal_events WHERE event_type='source_account_updated'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            2
        );
    }

    #[test]
    fn source_type_and_conversation_keys_are_isolated() {
        let db = Database::open_in_memory().unwrap();
        let now = Utc::now();
        ensure_source_account(
            db.connection(),
            "wechat-windows-4",
            "same-id",
            Some("微信账号"),
            &now.to_rfc3339(),
        )
        .unwrap();
        ensure_source_account(
            db.connection(),
            "other-source",
            "same-id",
            Some("其它账号"),
            &now.to_rfc3339(),
        )
        .unwrap();
        ensure_source_conversation(
            db.connection(),
            "other-source",
            "same-id",
            "chat-1",
            Some("群聊"),
            &now.to_rfc3339(),
        )
        .unwrap();

        assert_eq!(db.list_source_accounts().unwrap().len(), 2);
        assert_eq!(
            db.list_source_conversations(Some("wechat-windows-4"), Some("same-id"))
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            db.list_source_conversations(Some("other-source"), Some("same-id"))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn out_of_order_mapping_events_use_deterministic_last_write_wins() {
        let mut db = Database::open_in_memory().unwrap();
        let newer = JournalEvent {
            event_id: "event-z".into(),
            device_id: "device-b".into(),
            epoch: 1,
            seq: 2,
            logical_clock: 2,
            schema_version: 1,
            event_type: JournalEventType::SourceAccountUpdated,
            payload: serde_json::json!({
                "source_type": "wechat-windows-4",
                "source_account_id": "wxid_abc",
                "display_name": "新名称",
                "is_favorite": true,
            }),
            created_at: Utc::now(),
        };
        let older = JournalEvent {
            event_id: "event-a".into(),
            device_id: "device-a".into(),
            logical_clock: 1,
            ..newer.clone()
        };
        db.apply_source_account_updated_event(&newer).unwrap();
        db.apply_source_account_updated_event(&older).unwrap();
        let row = &db.list_source_accounts().unwrap()[0];
        assert_eq!(row.display_name.as_deref(), Some("新名称"));
        assert!(row.is_favorite);
    }
}
