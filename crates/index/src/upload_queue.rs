// ChatVault 上传来源与队列查询：校验受控副本，失败任务指数退避。
use crate::Database;
use chatvault_core::error::{ChatVaultError, Result};
use std::path::PathBuf;

/// 上传任务的最小执行信息。
pub struct PendingUpload {
    pub task_id: String,
    pub hash: String,
    pub record_id: String,
    pub original_name: String,
    pub size: u64,
}

impl Database {
    /// 返回所有当前可执行任务；按更新时间排序以免失败任务阻塞新文件。
    pub fn pending_uploads(&self) -> Result<Vec<PendingUpload>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.task_id,o.hash,t.record_id,r.original_name,o.size FROM upload_tasks t
             JOIN file_objects o ON t.object_id=o.object_id
             JOIN file_records r ON t.record_id=r.record_id
             WHERE t.status='queued' OR (t.status='retryable_failed' AND
             (julianday('now')-julianday(t.updated_at))*86400 >= MIN(3600,30*(1 << MIN(t.retry_count-1,7))))
             ORDER BY t.updated_at,t.task_id").map_err(db_error)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PendingUpload {
                    task_id: r.get(0)?,
                    hash: r.get(1)?,
                    record_id: r.get(2)?,
                    original_name: r.get(3)?,
                    size: r.get::<_, i64>(4)? as u64,
                })
            })
            .map_err(db_error)?;
        rows.map(|r| r.map_err(db_error)).collect()
    }

    /// 执行前再次确认暂停状态。
    pub fn task_is_runnable(&self, id: &str) -> Result<bool> {
        self.conn.query_row("SELECT EXISTS(SELECT 1 FROM upload_tasks WHERE task_id=?1 AND status IN ('queued','retryable_failed'))", [id], |r|r.get(0)).map_err(db_error)
    }

    /// 有副本则严格使用副本；未创建副本时读取原路径，两者都必须匹配入库哈希。
    pub fn upload_source(&self, id: &str) -> Result<PathBuf> {
        let (hash, cache, original, content_origin): (String, Option<String>, String, Option<String>) = self.conn.query_row(
            "SELECT o.hash,l.cache_path,l.original_path,l.content_origin FROM upload_tasks t JOIN file_objects o ON t.object_id=o.object_id
             LEFT JOIN local_files l ON t.record_id=l.record_id WHERE t.task_id=?1", [id],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(db_error)?;
        let selected = if content_origin.as_deref() == Some("decrypted") {
            cache.ok_or_else(|| ChatVaultError::FileNotFound {
                path: format!("任务 {id} 的明文缓存已回收，无法回退到微信密文"),
            })?
        } else {
            cache.unwrap_or(original)
        };
        let path = Some(PathBuf::from(&selected))
            .filter(|p| {
                std::fs::symlink_metadata(p)
                    .map(|metadata| {
                        metadata.file_type().is_file() && !metadata.file_type().is_symlink()
                    })
                    .unwrap_or(false)
            })
            .ok_or_else(|| ChatVaultError::FileNotFound {
                path: format!("任务 {id} 的上传来源已丢失：{selected}"),
            })?;
        chatvault_metadata::verify_file_hash(&path, &hash)?;
        Ok(path)
    }
}

/// 转换 SQLite 错误。
fn db_error(e: rusqlite::Error) -> ChatVaultError {
    ChatVaultError::Database(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn decrypted_upload_never_falls_back_to_ciphertext_source() {
        let db = Database::open_in_memory().unwrap();
        let source = std::env::temp_dir().join(format!(
            "chatvault-upload-cipher-{}.dat",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&source, b"ciphertext").unwrap();
        let plaintext = b"plaintext";
        let hash = chatvault_metadata::compute_blake3_bytes(plaintext).hex_hash;
        let object_id = format!("blake3:{hash}");
        let now = Utc::now().to_rfc3339();
        let conn = db.connection();
        conn.execute(
            "INSERT INTO file_objects (object_id,hash,size,mime,extension,created_at)
             VALUES (?1,?2,?3,'image/png','png',?4)",
            rusqlite::params![object_id, hash, plaintext.len() as i64, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_records
             (record_id,object_id,source_type,original_name,file_time,time_source,discovered_at,device_id)
             VALUES ('record-upload-cipher',?1,'wechat-windows-4','image.png',?2,'mtime',?2,'device')",
            rusqlite::params![object_id, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO local_files
             (record_id,original_path,cache_path,size,source_size,mtime_ms,availability,content_origin)
             VALUES ('record-upload-cipher',?1,NULL,?2,10,1,'remote_only','decrypted')",
            rusqlite::params![source.to_string_lossy().to_string(), plaintext.len() as i64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO upload_tasks (task_id,record_id,object_id,status,updated_at)
             VALUES ('task-upload-cipher','record-upload-cipher',?1,'queued',?2)",
            rusqlite::params![object_id, now],
        )
        .unwrap();

        assert!(matches!(
            db.upload_source("task-upload-cipher"),
            Err(ChatVaultError::FileNotFound { .. })
        ));
        let _ = std::fs::remove_file(source);
    }
}
