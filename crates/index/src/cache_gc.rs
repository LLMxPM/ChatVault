// ChatVault 缓存回收：保护未完成归档的引用，仅删除受控目录内的内容对象。
use crate::{cache_policy::MIB, Database};
use chatvault_core::error::{ChatVaultError, Result};
use std::{fs, time::SystemTime};

impl Database {
    /// 按保留时间和容量目标回收副本，返回释放字节数；不删除原附件。
    /// 与入库共用 SQLite 写锁，防止复制落盘与建立引用之间被回收。
    pub fn reclaim_cache(&mut self) -> Result<u64> {
        let policy = self.cache_policy()?;
        if !self.staging_dir.exists() {
            return Ok(0);
        }
        let directory = fs::canonicalize(&self.staging_dir)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let mut entries = Vec::new();
        let mut total = 0u64;
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            // 不跟随符号链接，不接触临时文件或未知文件。
            if !entry.file_type()?.is_file() || chatvault_metadata::validate_hash(&name).is_err() {
                continue;
            }
            let metadata = entry.metadata()?;
            total = total.saturating_add(metadata.len());
            let object = format!("blake3:{name}");
            let protected: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM local_files l JOIN file_records r USING(record_id)
                 WHERE r.object_id=?1 AND (
                   NOT EXISTS(SELECT 1 FROM upload_tasks t WHERE t.record_id=l.record_id AND t.status='backed_up')
                   OR EXISTS(SELECT 1 FROM upload_tasks t WHERE t.record_id=l.record_id AND t.status!='backed_up')
                   OR NOT EXISTS(SELECT 1 FROM journal_events j JOIN sync_cursors c ON c.device_id=j.device_id AND c.epoch=j.epoch
                     WHERE j.event_type='file_record_added' AND json_extract(j.payload,'$.record_id')=l.record_id AND c.last_contiguous_seq>=j.seq)
                 ))", [&object], |r| r.get(0)).map_err(db_error)?;
            if protected {
                continue;
            }
            let archived: Option<i64> = tx
                .query_row(
                    "SELECT MAX(unixepoch(updated_at)) FROM upload_tasks WHERE object_id=?1",
                    [&object],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            let timestamp = archived.map(|v| v.max(0) as u64).unwrap_or(
                metadata
                    .modified()?
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            entries.push((timestamp, entry.path(), object, metadata.len()));
        }
        entries.sort_by_key(|e| e.0);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut released = 0;
        for (timestamp, path, object, size) in entries {
            if now.saturating_sub(timestamp) < u64::from(policy.cache_retention_days) * 86400
                && total <= u64::from(policy.cache_max_mib) * MIB
            {
                continue;
            }
            // 删除目标必须是受控目录的直接子文件；上面的 file_type 已排除链接。
            if path.parent() != Some(directory.as_path()) {
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            tx.execute("UPDATE local_files SET cache_path=NULL WHERE record_id IN (SELECT record_id FROM file_records WHERE object_id=?1)", [&object]).map_err(db_error)?;
            total = total.saturating_sub(size);
            released += size;
        }
        tx.commit().map_err(db_error)?;
        Ok(released)
    }
}

/// 将 SQLite 错误映射为领域错误。
fn db_error(e: rusqlite::Error) -> ChatVaultError {
    ChatVaultError::Database(e.to_string())
}
