// ChatVault 同步协议校验：验证事件范围、对象引用和提交标记的一致性。
use chatvault_core::{
    error::{ChatVaultError, Result},
    models::{JournalEvent, JournalEventType},
};
use chatvault_metadata::{get_object_path, validate_hash};
use chatvault_webdav::{HeadProbe, RemoteVerifier, WebDavClient};

/// 校验单事件格式；允许未定义字段，但拒绝未知版本和不完整对象身份。
pub fn validate_event(ev: &JournalEvent) -> Result<()> {
    chatvault_metadata::validate_id(&ev.device_id)?;
    if ev.schema_version != 1 || ev.seq == 0 || ev.epoch == 0 || ev.event_id.is_empty() {
        return Err(ChatVaultError::Internal("事件版本或标识无效".into()));
    }

    match ev.event_type {
        JournalEventType::FileRecordAdded => {
            require_string(&ev.payload, "record_id")?;
            let source_type = require_string(&ev.payload, "source_type")?;
            validate_source_key(&source_type, "source_type")?;
            if let Some(account_id) = optional_string(&ev.payload, "source_account_id")? {
                validate_source_key(&account_id, "source_account_id")?;
            }
            if let Some(conversation_id) = optional_string(&ev.payload, "source_conversation_id")? {
                validate_source_key(&conversation_id, "source_conversation_id")?;
                if optional_string(&ev.payload, "source_account_id")?.is_none() {
                    return Err(ChatVaultError::Internal(
                        "source_conversation_id 不能脱离 source_account_id".into(),
                    ));
                }
            }
            let hash = require_string(&ev.payload, "hash")?;
            validate_hash(&hash)?;
            if ev.payload.get("object_id").and_then(|v| v.as_str())
                != Some(format!("blake3:{hash}").as_str())
                || ev.payload.get("size").and_then(|v| v.as_u64()).is_none()
            {
                return Err(ChatVaultError::Internal("事件对象身份或大小无效".into()));
            }
        }
        JournalEventType::FileRecordDeleted => {
            require_string(&ev.payload, "record_id")?;
        }
        JournalEventType::SourceAccountUpdated => {
            validate_mapping_payload(&ev.payload, false)?;
        }
        JournalEventType::SourceConversationUpdated => {
            validate_mapping_payload(&ev.payload, true)?;
        }
        JournalEventType::ObjectHidden
        | JournalEventType::ObjectRestored
        | JournalEventType::ObjectPurged => {
            let object_id = require_string(&ev.payload, "object_id")?;
            let hash = object_id
                .strip_prefix("blake3:")
                .ok_or_else(|| ChatVaultError::Internal("object_id 必须为 blake3: 前缀".into()))?;
            validate_hash(hash)?;
        }
    }
    Ok(())
}

/// 读取事件中的必需字符串字段。
fn require_string(payload: &serde_json::Value, field: &str) -> Result<String> {
    let value = payload
        .get(field)
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ChatVaultError::Internal(format!("事件缺少有效的 {field}")))?;
    Ok(value.to_string())
}

/// 读取可为空的字符串字段，并拒绝其它 JSON 类型。
fn optional_string(payload: &serde_json::Value, field: &str) -> Result<Option<String>> {
    match payload.get(field) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) if !value.trim().is_empty() => {
            Ok(Some(value.to_string()))
        }
        _ => Err(ChatVaultError::Internal(format!(
            "事件字段 {field} 类型无效"
        ))),
    }
}

/// 校验来源映射事件的键和用户维护字段。
fn validate_mapping_payload(payload: &serde_json::Value, conversation: bool) -> Result<()> {
    let source_type = require_string(payload, "source_type")?;
    let source_account_id = require_string(payload, "source_account_id")?;
    validate_source_key(&source_type, "source_type")?;
    validate_source_key(&source_account_id, "source_account_id")?;
    if conversation {
        let conversation_id = require_string(payload, "source_conversation_id")?;
        validate_source_key(&conversation_id, "source_conversation_id")?;
    }
    match payload.get("display_name") {
        Some(serde_json::Value::Null) | Some(serde_json::Value::String(_)) => {}
        _ => return Err(ChatVaultError::Internal("display_name 类型无效".into())),
    }
    if !payload
        .get("is_favorite")
        .and_then(|value| value.as_bool())
        .is_some()
    {
        return Err(ChatVaultError::Internal("is_favorite 类型无效".into()));
    }
    Ok(())
}

/// 校验来源键的非空和长度约束。
fn validate_source_key(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 512 {
        return Err(ChatVaultError::Internal(format!("{field} 无效")));
    }
    Ok(())
}

/// 确认每个新增来源引用的远端对象仍在且大小一致；缺失/截断阻止游标推进。
///
/// 信任模型：归档阶段已完整回读并校验 BLAKE3，WebDAV 是用户自有归档存储；
/// 同步只做存在/大小门禁，内容哈希在真正下载使用时再校验。无 Content-Length 时降级完整回读。
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
            let expected_size = event.payload["size"]
                .as_u64()
                .ok_or_else(|| ChatVaultError::Internal("事件缺少有效对象大小".into()))?;
            let actual_size = if let Some(size) = verified.get(hash) {
                *size
            } else {
                let size = verify_one_reference(client, vault, hash, expected_size).await?;
                verified.insert(hash, size);
                size
            };
            if actual_size != expected_size {
                return Err(ChatVaultError::Internal(
                    "事件声明的对象大小与远端内容不一致".into(),
                ));
            }
        }
    }
    Ok(())
}

/// 轻量校验单个被引用对象；返回远端确认的字节大小。
async fn verify_one_reference(
    client: &WebDavClient,
    vault: &str,
    hash: &str,
    expected_size: u64,
) -> Result<u64> {
    let path = get_object_path(vault, hash);
    match client.head_object(&path).await? {
        HeadProbe::NotFound => Err(ChatVaultError::WebDav(format!("远端对象缺失: {path}"))),
        HeadProbe::Exists {
            content_length: Some(len),
        } if len == expected_size => Ok(len),
        HeadProbe::Exists {
            content_length: Some(len),
        } => Err(ChatVaultError::Internal(format!(
            "远端对象大小 {len} 与事件声明 {expected_size} 不一致: {path}"
        ))),
        // 部分服务 HEAD 不带 Content-Length：降级完整回读，保证协议仍成立。
        HeadProbe::Exists {
            content_length: None,
        } => {
            RemoteVerifier::new(client)
                .verify_remote_size(&path, hash)
                .await
        }
    }
}
