// ChatVault 日志发布：本机事件打包为不可变分片并发布 commit 标记
//
// 发布顺序：对象已就绪 → 上传 journal 分片 → 校验 → 上传 commit 标记 → 更新游标

use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::{DeviceInfo, JournalEvent};
use chatvault_index::Database;
use chatvault_metadata::{get_commit_path, get_device_path, get_journal_segment_path};
use chatvault_webdav::WebDavClient;

/// 日志发布器
pub struct JournalPublisher<'a> {
    client: &'a WebDavClient,
    vault_id: String,
    device_id: String,
    epoch: u64,
}

impl<'a> JournalPublisher<'a> {
    /// 创建发布器；epoch 从本地设置读取，默认 1
    pub fn new(client: &'a WebDavClient, vault_id: &str, device_id: &str, epoch: u64) -> Self {
        Self {
            client,
            vault_id: vault_id.to_string(),
            device_id: device_id.to_string(),
            epoch: epoch.max(1),
        }
    }

    /// 读取本地当前 epoch（设置 key: epoch）
    pub fn load_epoch(db: &Database, device_id: &str) -> u64 {
        let key = format!("epoch_{}", device_id);
        db.get_setting(&key)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
    }

    /// 保存本地 epoch
    pub fn save_epoch(db: &mut Database, device_id: &str, epoch: u64) -> Result<()> {
        db.set_setting(&format!("epoch_{}", device_id), &epoch.to_string())
    }

    /// 发布所有未推送的本机事件
    ///
    /// 1. 取本地 journal_events 中尚未有 commit 的连续段
    /// 2. 序列化为 JSONL，计算 BLAKE3
    /// 3. 上传 journal 分片
    /// 4. 上传 commit 标记
    /// 5. 更新本机游标与设备注册信息
    pub async fn publish_pending_events(&self, db: &mut Database) -> Result<u64> {
        chatvault_metadata::validate_id(&self.device_id)?;
        db.check_remote_binding(self.client.storage_identity(), &self.vault_id)?;
        chatvault_webdav::ensure_vault_config(
            self.client,
            &chatvault_core::models::VaultConfig {
                vault_id: self.vault_id.clone(),
                format_version: 1,
                hash_algorithm: "blake3".into(),
                created_at: chrono::Utc::now(),
            },
        )
        .await?;
        db.bind_remote(self.client.storage_identity(), &self.vault_id)?;
        let last_pushed = db.cursor_seq(&self.device_id, self.epoch)?;
        // 即使没有新事件也修复设备注册；游标已持久化后注册失败可以安全重试。
        self.register_device(db, last_pushed).await?;
        let pending_key = format!("pending_segment_{}_{}", self.device_id, self.epoch);
        let events: Vec<JournalEvent> = db.atomic(|db| {
            db.connection().execute("UPDATE app_settings SET value=value WHERE key='device_id'", [])
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            if let Some(saved) = db.get_setting(&pending_key)? { return Ok(serde_json::from_str(&saved)?); }
            let current = db.cursor_seq(&self.device_id,self.epoch)?;
            let candidates = db.list_journal_events_epoch(&self.device_id, self.epoch, current, 500)?;
            let mut ready = Vec::new();
            for event in candidates {
                crate::validation::validate_event(&event)?;
                if event.seq != current + ready.len() as u64 + 1 {
                    return Err(ChatVaultError::Internal("本机事件序列存在缺口".into()));
                }
                if event.event_type == chatvault_core::models::JournalEventType::FileRecordAdded {
                    let backed_up: bool = db.connection().query_row(
                        "SELECT EXISTS(SELECT 1 FROM upload_tasks WHERE object_id=?1 AND status='backed_up')",
                        [event.payload["object_id"].as_str().unwrap()], |r|r.get(0))
                        .map_err(|e| ChatVaultError::Database(e.to_string()))?;
                    if !backed_up { break; }
                }
                ready.push(event);
            }
            if !ready.is_empty() { db.set_setting(&pending_key, &serde_json::to_string(&ready)?)?; }
            Ok(ready)
        })?;
        if events.is_empty() {
            return Ok(last_pushed);
        }
        crate::validation::verify_references(self.client, &self.vault_id, &events).await?;
        let first_seq = events.first().unwrap().seq;
        let last_seq = events.last().unwrap().seq;
        let mut jsonl = String::new();
        for event in &events {
            jsonl.push_str(&serde_json::to_string(event)?);
            jsonl.push('\n');
        }
        let segment_hash = blake3::hash(jsonl.as_bytes()).to_hex().to_string();
        let journal_path = get_journal_segment_path(
            &self.vault_id,
            &self.device_id,
            self.epoch,
            last_seq,
            &segment_hash,
        );
        self.client
            .upload_immutable(jsonl.as_bytes(), &journal_path)
            .await?;
        let commit = serde_json::json!({
            "device_id":self.device_id, "epoch":self.epoch, "seq":last_seq,
            "journal_path":journal_path, "journal_length":jsonl.len(),
            "journal_hash":format!("blake3:{}",segment_hash), "event_count":events.len(),
            "first_seq":first_seq, "last_seq":last_seq,
            "created_at":events.last().unwrap().created_at.to_rfc3339(),
        });
        self.client
            .upload_immutable(
                &serde_json::to_vec_pretty(&commit)?,
                &get_commit_path(&self.vault_id, &self.device_id, self.epoch, last_seq),
            )
            .await?;
        let saved = serde_json::to_string(&events)?;
        db.atomic(|db| {
            db.upsert_sync_cursor(&self.device_id, self.epoch, last_seq)?;
            db.connection()
                .execute(
                    "DELETE FROM app_settings WHERE key=?1 AND value=?2",
                    [&pending_key, &saved],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            Ok(())
        })?;
        self.register_device(db, last_seq).await?;
        Ok(last_seq)
    }

    /// 注册信息是可重试步骤；首次注册可先于 commit，读取方只消费有效提交。
    async fn register_device(&self, db: &mut Database, seq: u64) -> Result<()> {
        let device = DeviceInfo {
            device_id: self.device_id.clone(),
            display_name: None,
            epoch: self.epoch,
            last_seq: seq,
            updated_at: chrono::Utc::now(),
        };
        self.client
            .upload_bytes(
                serde_json::to_vec_pretty(&device)?,
                &get_device_path(&self.vault_id, &self.device_id),
            )
            .await?;
        db.upsert_known_device(&device)
    }
}

/// 将本地未发布事件一次性推送（便捷封装）
pub async fn publish_pending_events(
    client: &WebDavClient,
    db: &mut Database,
    vault_id: &str,
    device_id: &str,
) -> Result<u64> {
    let epoch = JournalPublisher::load_epoch(db, device_id);
    let publisher = JournalPublisher::new(client, vault_id, device_id, epoch);
    loop {
        let before = db.cursor_seq(device_id, epoch)?;
        let after = publisher.publish_pending_events(db).await?;
        if after <= before {
            return Ok(after);
        }
    }
}

/// 校验分片内容哈希与 commit 声明一致
pub fn verify_segment_bytes(bytes: &[u8], expected_hex: &str) -> Result<()> {
    let clean = expected_hex.trim_start_matches("blake3:");
    let actual = blake3::hash(bytes).to_hex().to_string();
    if actual.eq_ignore_ascii_case(clean) {
        Ok(())
    } else {
        Err(ChatVaultError::HashMismatch {
            expected: clean.to_string(),
            actual,
        })
    }
}

/// 从 JSONL 解析事件列表
pub fn parse_journal_jsonl(text: &str) -> Result<Vec<JournalEvent>> {
    let mut events = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let ev: JournalEvent = serde_json::from_str(trimmed)?;
        events.push(ev);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatvault_core::models::{JournalEvent, JournalEventType};
    use chrono::Utc;

    fn sample_event(seq: u64) -> JournalEvent {
        JournalEvent {
            event_id: format!("evt-{}", seq),
            device_id: "pc-a".into(),
            epoch: 1,
            seq,
            logical_clock: seq,
            schema_version: 1,
            event_type: JournalEventType::FileRecordAdded,
            payload: serde_json::json!({"seq": seq}),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_parse_journal_jsonl_skips_blank_lines() {
        let mut text = String::new();
        text.push_str(&serde_json::to_string(&sample_event(1)).unwrap());
        text.push_str("\n\n");
        text.push_str(&serde_json::to_string(&sample_event(2)).unwrap());
        text.push('\n');

        let events = parse_journal_jsonl(&text).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[1].seq, 2);
    }

    #[test]
    fn test_verify_segment_bytes_accepts_correct_hash() {
        let payload = b"line1\nline2\n";
        let hex = blake3::hash(payload).to_hex().to_string();
        assert!(verify_segment_bytes(payload, &hex).is_ok());
        assert!(verify_segment_bytes(payload, &format!("blake3:{}", hex)).is_ok());
    }

    #[test]
    fn test_epoch_save_load() {
        let mut db = Database::open_in_memory().unwrap();
        assert_eq!(JournalPublisher::load_epoch(&db, "pc-a"), 1);
        JournalPublisher::save_epoch(&mut db, "pc-a", 7).unwrap();
        assert_eq!(JournalPublisher::load_epoch(&db, "pc-a"), 7);
        // 未设置的设备仍为 1
        assert_eq!(JournalPublisher::load_epoch(&db, "pc-b"), 1);
    }

    #[test]
    fn test_list_unpublished_journal_events() {
        let mut db = Database::open_in_memory().unwrap();
        for seq in 1..=3u64 {
            db.insert_journal_event_if_absent(&sample_event(seq))
                .unwrap();
        }
        let all = db.list_unpublished_journal_events("pc-a", 0, 10).unwrap();
        assert_eq!(all.len(), 3);
        let after = db.list_unpublished_journal_events("pc-a", 2, 10).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].seq, 3);
    }
}
