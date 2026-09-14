// ChatVault 远端事件拉取与本地合并
//
// 读取 devices/*.json 发现其他设备，列出其 commit 目录中已发布的分片，
// 按 seq 升序消费连续可应用的 commit，校验 journal 哈希后幂等应用到本地 SQLite。

use crate::publish::{parse_journal_jsonl, verify_segment_bytes};
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::{DeviceInfo, JournalEvent, JournalEventType};
use chatvault_index::Database;
use chatvault_metadata::{get_commit_dir, get_devices_dir, get_journal_segment_path};
use chatvault_webdav::WebDavClient;
use chrono::{DateTime, Utc};

/// commit 标记结构
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommitMarker {
    pub device_id: String,
    pub epoch: u64,
    pub seq: u64,
    pub journal_path: String,
    pub journal_length: u64,
    pub journal_hash: String,
    pub event_count: u64,
    pub first_seq: u64,
    pub last_seq: u64,
}

/// 拉取所有远端设备事件并应用到本地
///
/// 输入:
///   - `client`: WebDAV 客户端
///   - `db`: 本地数据库
///   - `vault_id`: 资料库 ID
///   - `local_device_id`: 本机设备 ID（跳过自己的日志）
///     输出: `Result<usize>` 应用的新事件数
pub async fn pull_and_apply(
    client: &WebDavClient,
    db: &mut Database,
    vault_id: &str,
    local_device_id: &str,
) -> Result<usize> {
    chatvault_metadata::validate_id(local_device_id)?;
    db.check_remote_binding(client.storage_identity(), vault_id)?;
    chatvault_webdav::load_vault_config(client, vault_id).await?;
    db.bind_remote(client.storage_identity(), vault_id)?;
    let devices = list_remote_devices(client, db, vault_id).await?;
    let mut applied_total = 0usize;

    for device in devices {
        if device.device_id == local_device_id {
            continue;
        }
        applied_total += pull_device_events(client, db, vault_id, &device.device_id).await?;
    }

    Ok(applied_total)
}

/// 列出远端设备注册信息，并同步到本地 known_devices
async fn list_remote_devices(
    client: &WebDavClient,
    db: &mut Database,
    vault_id: &str,
) -> Result<Vec<DeviceInfo>> {
    let dir = get_devices_dir(vault_id);
    let paths = client.list_dir(&dir).await?;

    let mut devices = Vec::new();
    for p in paths {
        if !p.ends_with(".json") {
            continue;
        }
        let info = download_json::<DeviceInfo>(client, &p).await?;
        chatvault_metadata::validate_id(&info.device_id)?;
        if info.epoch == 0 || p != chatvault_metadata::get_device_path(vault_id, &info.device_id) {
            return Err(ChatVaultError::Internal("设备注册路径或 epoch 无效".into()));
        }
        db.upsert_known_device(&info)?;
        devices.push(info);
    }
    Ok(devices)
}

/// 拉取单个设备从本地游标之后的全部连续 commit 并应用
///
/// 关键：不假设 commit 文件名按 1,2,3... 连续。发布端批量打包后
/// 可能只生成 last_seq.json（例如 5.json）。正确做法是列出
/// commits/<device>/<epoch>/ 下所有 commit，按 seq 升序消费。
async fn pull_device_events(
    client: &WebDavClient,
    db: &mut Database,
    vault_id: &str,
    device_id: &str,
) -> Result<usize> {
    let root = format!("{vault_id}/commits/{device_id}");
    let mut epochs: Vec<u64> = client
        .list_dir(&root)
        .await?
        .iter()
        .filter_map(|p| p.rsplit('/').next()?.parse().ok())
        .collect();
    epochs.sort_unstable();
    epochs.dedup();
    let mut applied = 0;
    for epoch in epochs {
        if epoch == 0 {
            return Err(ChatVaultError::Internal("远端 epoch 无效".into()));
        }
        applied += pull_epoch_events(client, db, vault_id, device_id, epoch).await?;
    }
    Ok(applied)
}

/// 按单独 epoch 重放，换机恢复同时保留历史 epoch 的全部来源。
async fn pull_epoch_events(
    client: &WebDavClient,
    db: &mut Database,
    vault_id: &str,
    device_id: &str,
    epoch: u64,
) -> Result<usize> {
    let mut last_applied = db.cursor_seq(device_id, epoch)?;

    // 列出该设备/epoch 下全部 commit 文件，解析 seq 并升序
    let commit_dir = get_commit_dir(vault_id, device_id, epoch);
    let commit_paths = client.list_dir(&commit_dir).await?;

    let mut commits_with_seq: Vec<(u64, String)> = Vec::new();
    for path in commit_paths {
        if !path.ends_with(".json") {
            continue;
        }
        if let Some(seq) = seq_from_commit_path(&path) {
            commits_with_seq.push((seq, path));
        }
    }
    commits_with_seq.sort_by_key(|(seq, _)| *seq);

    let mut applied = 0usize;

    for (file_seq, commit_path) in commits_with_seq {
        // 已应用过的 commit 跳过
        if file_seq <= last_applied {
            continue;
        }

        let commit = download_json::<CommitMarker>(client, &commit_path).await?;
        if commit.device_id != device_id
            || commit.epoch != epoch
            || commit.seq != file_seq
            || commit.last_seq != file_seq
            || commit.first_seq > last_applied + 1
            || commit.first_seq == 0
            || commit.first_seq > commit.last_seq
        {
            return Err(ChatVaultError::Internal(format!(
                "提交身份或连续范围不正确: {commit_path}"
            )));
        }
        let hash = commit
            .journal_hash
            .strip_prefix("blake3:")
            .ok_or_else(|| ChatVaultError::Internal("分片哈希算法不支持".into()))?;
        chatvault_metadata::validate_hash(hash)?;
        let expected_path = get_journal_segment_path(vault_id, device_id, epoch, file_seq, hash);
        if commit.journal_path != expected_path {
            return Err(ChatVaultError::Internal("日志路径与提交标记不一致".into()));
        }
        let bytes = client
            .get_stream(&expected_path)
            .await?
            .bytes()
            .await
            .map_err(|e| ChatVaultError::WebDav(e.to_string()))?;
        verify_segment_bytes(&bytes, &commit.journal_hash)?;
        if bytes.len() as u64 != commit.journal_length {
            return Err(ChatVaultError::Internal("分片长度不一致".into()));
        }
        let text =
            std::str::from_utf8(&bytes).map_err(|e| ChatVaultError::Internal(e.to_string()))?;
        let events = parse_journal_jsonl(text)?;
        if events.len() as u64 != commit.event_count
            || commit.event_count != commit.last_seq - commit.first_seq + 1
        {
            return Err(ChatVaultError::Internal("分片事件数量不一致".into()));
        }
        for (index, event) in events.iter().enumerate() {
            if event.device_id != device_id
                || event.epoch != epoch
                || event.seq != commit.first_seq + index as u64
            {
                return Err(ChatVaultError::Internal(
                    "分片事件不连续或身份不一致".into(),
                ));
            }
        }
        crate::validation::verify_references(client, vault_id, &events).await?;
        let new_count = db.atomic(|db| {
            let mut count = 0;
            for event in &events {
                if apply_event(db, event)? {
                    count += 1;
                }
            }
            db.upsert_sync_cursor(device_id, epoch, commit.last_seq)?;
            Ok(count)
        })?;
        last_applied = commit.last_seq;
        applied += new_count;
    }

    Ok(applied)
}

/// 从 commit 路径提取 seq，例如 `.../commits/dev1/1/5.json` → Some(5)
fn seq_from_commit_path(path: &str) -> Option<u64> {
    let file_name = path.rsplit('/').next()?;
    let stem = file_name.strip_suffix(".json")?;
    stem.parse().ok()
}

/// 幂等应用单个事件到本地索引
///
/// 返回是否真正插入了新数据
pub fn apply_event(db: &mut Database, ev: &JournalEvent) -> Result<bool> {
    crate::validation::validate_event(ev)?;
    db.atomic(|db| {
        // 先插入日志以检测设备序列冲突，错误时所有数据一起回滚。
        db.insert_journal_event_if_absent(ev)?;
        if db.event_already_applied(&ev.event_id)? {
            return Ok(false);
        }
        match ev.event_type {
            JournalEventType::FileRecordAdded => db.apply_file_record_added_event(ev)?,
            JournalEventType::FileRecordDeleted => db.apply_file_record_deleted_event(ev)?,
            JournalEventType::SourceAccountUpdated => db.apply_source_account_updated_event(ev)?,
            JournalEventType::SourceConversationUpdated => {
                db.apply_source_conversation_updated_event(ev)?
            }
            JournalEventType::ObjectHidden => db.apply_object_hidden_event(ev)?,
            JournalEventType::ObjectRestored => db.apply_object_restored_event(ev)?,
            JournalEventType::ObjectPurged => db.apply_object_purged_event(ev)?,
        }
        db.mark_event_applied(&ev.event_id)?;
        Ok(true)
    })
}

/// 空索引恢复：先从远端设备列表拉全部事件，再返回可查询记录数
pub async fn restore_from_remote(
    client: &WebDavClient,
    db: &mut Database,
    vault_id: &str,
    local_device_id: &str,
) -> Result<RestoreReport> {
    let applied = pull_and_apply(client, db, vault_id, local_device_id).await?;
    let stats = db.get_stats()?;
    Ok(RestoreReport {
        applied_events: applied,
        total_records: stats.total_records,
        total_objects: stats.total_objects,
    })
}

/// 恢复报告
#[derive(Debug, Clone, Default)]
pub struct RestoreReport {
    pub applied_events: usize,
    pub total_records: usize,
    pub total_objects: usize,
}

async fn download_json<T: serde::de::DeserializeOwned>(
    client: &WebDavClient,
    path: &str,
) -> Result<T> {
    let resp = client.get_stream(path).await?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ChatVaultError::WebDav(e.to_string()))?;
    let value: T = serde_json::from_slice(&bytes)?;
    Ok(value)
}

/// 解析 RFC3339 时间（容错）
pub fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seq_from_commit_path() {
        assert_eq!(
            seq_from_commit_path("chatvault-v/commits/dev1/1/5.json"),
            Some(5)
        );
        assert_eq!(
            seq_from_commit_path("chatvault-v/commits/dev1/1/1.json"),
            Some(1)
        );
        assert_eq!(seq_from_commit_path("chatvault-v/commits/dev1/1/"), None);
        assert_eq!(seq_from_commit_path("foo/bar.txt"), None);
    }

    #[test]
    fn test_apply_event_idempotent() {
        use chatvault_core::models::{JournalEvent, JournalEventType};
        use chrono::Utc;

        let mut db = Database::open_in_memory().unwrap();
        let ev = JournalEvent {
            event_id: "evt-1".into(),
            device_id: "remote-pc".into(),
            epoch: 1,
            seq: 1,
            logical_clock: 1,
            schema_version: 1,
            event_type: JournalEventType::FileRecordAdded,
            payload: serde_json::json!({
                "object_id": format!("blake3:{}", "a".repeat(64)),
                "record_id": "rec-1",
                "hash": "a".repeat(64),
                "size": 10,
                "mime": "application/pdf",
                "extension": "pdf",
                "object_created_at": Utc::now().to_rfc3339(),
                "source_type": "wechat-windows",
                "source_account_id": "wxid_x",
                "source_conversation_id": null,
                "original_name": "测试文档.pdf",
                "file_time": Utc::now().to_rfc3339(),
                "time_source": "mtime",
                "discovered_at": Utc::now().to_rfc3339(),
                "device_id": "remote-pc",
            }),
            created_at: Utc::now(),
        };

        let first = apply_event(&mut db, &ev).unwrap();
        assert!(first);
        let second = apply_event(&mut db, &ev).unwrap();
        assert!(!second);

        let stats = db.get_stats().unwrap();
        assert_eq!(stats.total_records, 1);
        assert_eq!(stats.total_objects, 1);
        assert!(db.event_already_applied("evt-1").unwrap());
    }

    #[test]
    fn test_parse_journal_jsonl_roundtrip() {
        use chatvault_core::models::{JournalEvent, JournalEventType};
        use chrono::Utc;

        let ev = JournalEvent {
            event_id: "e1".into(),
            device_id: "d1".into(),
            epoch: 1,
            seq: 3,
            logical_clock: 3,
            schema_version: 1,
            event_type: JournalEventType::FileRecordAdded,
            payload: serde_json::json!({"hello": "world"}),
            created_at: Utc::now(),
        };
        let line = serde_json::to_string(&ev).unwrap();
        let mut jsonl = String::new();
        jsonl.push_str(&line);
        jsonl.push('\n');

        let parsed = parse_journal_jsonl(&jsonl).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].event_id, "e1");
        assert_eq!(parsed[0].seq, 3);
    }

    #[test]
    fn test_verify_segment_bytes() {
        let data = b"hello chatvault";
        let hash = blake3::hash(data).to_hex().to_string();
        assert!(verify_segment_bytes(data, &format!("blake3:{}", hash)).is_ok());
        assert!(verify_segment_bytes(data, "deadbeef").is_err());
    }

    #[test]
    fn test_source_mapping_events_are_idempotent_and_lww() {
        use chatvault_core::models::{JournalEvent, JournalEventType};
        use chrono::Utc;

        let mut db = Database::open_in_memory().unwrap();
        let newer = JournalEvent {
            event_id: "mapping-new".into(),
            device_id: "device-b".into(),
            epoch: 1,
            seq: 1,
            logical_clock: 2,
            schema_version: 1,
            event_type: JournalEventType::SourceAccountUpdated,
            payload: serde_json::json!({
                "source_type": "wechat-windows-4",
                "source_account_id": "wxid_test",
                "display_name": "新名称",
                "is_favorite": true,
            }),
            created_at: Utc::now(),
        };
        let older = JournalEvent {
            event_id: "mapping-old".into(),
            device_id: "device-a".into(),
            seq: 1,
            logical_clock: 1,
            ..newer.clone()
        };

        assert!(apply_event(&mut db, &newer).unwrap());
        assert!(!apply_event(&mut db, &newer).unwrap());
        assert!(apply_event(&mut db, &older).unwrap());
        let account = &db.list_source_accounts().unwrap()[0];
        assert_eq!(account.display_name.as_deref(), Some("新名称"));
        assert!(account.is_favorite);

        let conversation = JournalEvent {
            event_id: "conversation-1".into(),
            device_id: "device-a".into(),
            epoch: 1,
            seq: 2,
            logical_clock: 3,
            schema_version: 1,
            event_type: JournalEventType::SourceConversationUpdated,
            payload: serde_json::json!({
                "source_type": "wechat-windows-4",
                "source_account_id": "wxid_test",
                "source_conversation_id": "chat-123",
                "display_name": null,
                "is_favorite": false,
            }),
            created_at: Utc::now(),
        };
        assert!(apply_event(&mut db, &conversation).unwrap());
        assert_eq!(
            db.list_source_conversations(Some("wechat-windows-4"), Some("wxid_test"))
                .unwrap()[0]
                .effective_name,
            "chat-123"
        );
    }
}
