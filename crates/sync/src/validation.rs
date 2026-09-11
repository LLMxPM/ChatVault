// ChatVault 同步协议校验：验证事件范围、对象引用和提交标记的一致性。
use chatvault_core::{
    error::{ChatVaultError, Result},
    models::{JournalEvent, JournalEventType},
};
use chatvault_metadata::{get_object_path, validate_hash};
use chatvault_webdav::{RemoteVerifier, WebDavClient};

/// 校验单事件格式；允许未定义字段，但拒绝未知版本和不完整对象身份。
pub fn validate_event(ev: &JournalEvent) -> Result<()> {
    chatvault_metadata::validate_id(&ev.device_id)?;
    if ev.schema_version != 1
        || ev.seq == 0
        || ev.epoch == 0
        || ev.event_id.is_empty()
        || ev
            .payload
            .get("record_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
    {
        return Err(ChatVaultError::Internal("事件版本或标识无效".into()));
    }
    if ev.event_type == JournalEventType::FileRecordAdded {
        let hash = ev
            .payload
            .get("hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        validate_hash(hash)?;
        if ev.payload.get("object_id").and_then(|v| v.as_str())
            != Some(format!("blake3:{hash}").as_str())
            || ev.payload.get("size").and_then(|v| v.as_u64()).is_none()
        {
            return Err(ChatVaultError::Internal("事件对象身份或大小无效".into()));
        }
    }
    Ok(())
}

/// 确认每个新增来源引用的远端内容已完整可读；缺失和损坏均阻止游标推进。
pub async fn verify_references(
    client: &WebDavClient,
    vault: &str,
    events: &[JournalEvent],
) -> Result<()> {
    let mut verified = std::collections::HashMap::new();
    for event in events {
        validate_event(event)?;
        if event.event_type == JournalEventType::FileRecordAdded {
            let hash = event.payload["hash"].as_str().unwrap();
            let actual_size = if let Some(size) = verified.get(hash) {
                *size
            } else {
                let size = RemoteVerifier::new(client)
                    .verify_remote_size(&get_object_path(vault, hash), hash)
                    .await?;
                verified.insert(hash, size);
                size
            };
            if event.payload["size"].as_u64() != Some(actual_size) {
                return Err(ChatVaultError::Internal(
                    "事件声明的对象大小与远端内容不一致".into(),
                ));
            }
        }
    }
    Ok(())
}
