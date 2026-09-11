// ChatVault 旧索引远端绑定：已有同步状态必须先证明属于当前远端，防止升级后误绑定空 Vault。
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_index::Database;
use chatvault_webdav::{RemoteVerifier, WebDavClient};

/// 验证旧库已归档对象和连续游标对应提交，再持久化唯一绑定。
pub async fn bind_checked(client: &WebDavClient, db: &mut Database, vault: &str) -> Result<()> {
    if db.get_setting("remote_binding")?.is_none() {
        let hashes = {
            let mut stmt=db.connection().prepare("SELECT DISTINCT o.hash FROM file_objects o JOIN upload_tasks t ON t.object_id=o.object_id WHERE t.status='backed_up'").map_err(db_error)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(db_error)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(db_error)?
        };
        for hash in hashes {
            chatvault_metadata::validate_hash(&hash)?;
            RemoteVerifier::new(client)
                .verify_remote_hash(&chatvault_metadata::get_object_path(vault, &hash), &hash)
                .await
                .map_err(|e| {
                    ChatVaultError::WebDav(format!(
                        "旧索引已有归档记录，请连接原资料库；对象验证失败: {e}"
                    ))
                })?;
        }
        let cursors = {
            let mut stmt=db.connection().prepare("SELECT device_id,epoch,last_contiguous_seq FROM sync_cursors WHERE last_contiguous_seq>0").map_err(db_error)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, u64>(1)?,
                        r.get::<_, u64>(2)?,
                    ))
                })
                .map_err(db_error)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(db_error)?
        };
        for (device, epoch, seq) in cursors {
            let path = chatvault_metadata::get_commit_path(vault, &device, epoch, seq);
            let bytes = client
                .get_stream(&path)
                .await?
                .bytes()
                .await
                .map_err(|e| ChatVaultError::WebDav(e.to_string()))?;
            let marker: crate::apply::CommitMarker = serde_json::from_slice(&bytes)?;
            if marker.device_id != device || marker.epoch != epoch || marker.last_seq != seq {
                return Err(ChatVaultError::WebDav(
                    "旧索引游标与目标资料库不一致，请连接原 Vault".into(),
                ));
            }
        }
    }
    db.bind_remote(client.storage_identity(), vault)
}

/// 转换数据库错误。
fn db_error(e: rusqlite::Error) -> ChatVaultError {
    ChatVaultError::Database(e.to_string())
}
