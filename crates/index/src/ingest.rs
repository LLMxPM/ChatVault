// ChatVault 文件入库：受控暂存、增量发现、内容去重与本机事件原子写入。
use crate::db::*;
use chatvault_core::{
    error::{ChatVaultError, Result},
    models::DiscoveredFile,
};
use chrono::Utc;
use rusqlite::params;
use std::path::Path;
use uuid::Uuid;

impl Database {
    /// 增量入库单个发现的文件
    ///
    /// 职责: 检查文件是否已索引过；若未变则跳过；若为新文件则计算 BLAKE3、去重入库并插入全文索引
    /// 输入:
    ///   - `file`: 适配器发现的文件元信息
    ///   - `device_id`: 当前执行操作的设备 ID
    /// 输出: `Result<IngestResult>`
    /// 关键约束:
    ///   - 整个入库过程在 SQLite 事务中执行，保证原子性
    ///   - 相同内容复用已有的 file_objects，仅追加 file_records
    pub fn ingest_file(&mut self, file: &DiscoveredFile, device_id: &str) -> Result<IngestResult> {
        let mut file = file.clone();
        let source = std::fs::canonicalize(&file.absolute_path)?;
        let is_index = self
            .path()
            .map(|db| {
                source == db
                    || source == std::path::PathBuf::from(format!("{}-wal", db.display()))
                    || source == std::path::PathBuf::from(format!("{}-shm", db.display()))
            })
            .unwrap_or(false);
        if is_index || source.starts_with(&self.staging_dir) {
            return Ok(IngestResult::Skipped {
                path: file.absolute_path.clone(),
            });
        }
        let canonical = source.to_string_lossy();
        file.absolute_path = if let Some(unc) = canonical.strip_prefix("\\\\?\\UNC\\") {
            format!("\\\\{unc}")
        } else {
            canonical
                .strip_prefix("\\\\?\\")
                .unwrap_or(&canonical)
                .to_string()
        };
        let metadata = std::fs::metadata(&source)?;
        file.modified_time = metadata.modified()?.into();
        file.file_size = metadata.len();
        let abs_path = &file.absolute_path;
        let mtime_ms = file.modified_time.timestamp_millis();

        // 1. 检查本机文件表，若路径和 mtime 完全一致则判定为已入库，跳过
        let existing_mtime: Option<(i64, i64, Option<String>)> = {
            let mut check_stmt = self
                .conn
                .prepare("SELECT mtime_ms, size, cache_path FROM local_files WHERE original_path = ?1 ORDER BY rowid DESC LIMIT 1")
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            check_stmt
                .query_row(params![abs_path], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .ok()
        };

        if let Some((prev_mtime, prev_size, Some(cache))) = existing_mtime {
            if prev_mtime == mtime_ms
                && prev_size == file.file_size as i64
                && Path::new(&cache).exists()
            {
                return Ok(IngestResult::Skipped {
                    path: abs_path.clone(),
                });
            }
        }

        // 2. 文件是新的或 mtime 发生了变化，计算 BLAKE3 哈希
        let (cache_path, hash_res, modified) =
            chatvault_metadata::staging::stage_file(&source, &self.staging_dir)?;
        file.file_size = hash_res.bytes_read;
        file.modified_time = modified.into();
        let mtime_ms = file.modified_time.timestamp_millis();
        let object_id = hash_res.object_id;
        let hex_hash = hash_res.hex_hash;
        let extension = Path::new(abs_path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let mime = format!("application/{}", extension); // 基础推断

        let epoch = self
            .get_setting(&format!("epoch_{}", device_id))?
            .and_then(|v| v.parse().ok())
            .unwrap_or(1u64);
        // 取得写锁之后分配序号，避免桌面和计划任务并发扫描产生相同序号。
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let next_seq: i64 = tx.query_row("SELECT COALESCE(MAX(seq), 0)+1 FROM journal_events WHERE device_id=?1 AND epoch=?2", params![device_id, epoch], |r| r.get(0))
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let logical_clock: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(logical_clock), 0)+1 FROM journal_events",
                [],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        // 重新检查发现键，处理等待写锁期间另一个扫描进程已入库的情况。
        let indexed: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM local_files l JOIN file_records r ON l.record_id=r.record_id WHERE l.original_path=?1 AND l.mtime_ms=?2 AND l.size=?3 AND r.object_id=?4)",
            params![abs_path, mtime_ms, file.file_size as i64, object_id], |r| r.get(0))
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        if indexed {
            tx.execute("UPDATE local_files SET cache_path=?1 WHERE original_path=?2 AND mtime_ms=?3 AND record_id IN (SELECT record_id FROM file_records WHERE object_id=?4)",
                params![cache_path.to_string_lossy(),abs_path,mtime_ms,object_id]).map_err(|e|ChatVaultError::Database(e.to_string()))?;
            tx.commit()
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            return Ok(IngestResult::Skipped {
                path: abs_path.clone(),
            });
        }
        // 3.1 尝试插入 file_objects
        let is_new_object = {
            let mut obj_check = tx
                .prepare("SELECT count(*) FROM file_objects WHERE object_id = ?1")
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            let count: i64 = obj_check
                .query_row(params![object_id], |row| row.get(0))
                .unwrap_or(0);
            count == 0
        };

        if is_new_object {
            tx.execute(
                "INSERT INTO file_objects (object_id, hash, size, mime, extension, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    object_id,
                    hex_hash,
                    file.file_size as i64,
                    mime,
                    extension,
                    Utc::now().to_rfc3339()
                ],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        }

        // 3.2 插入新的 file_records
        let record_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO file_records (
                record_id, object_id, source, account_id, conversation_id,
                original_name, file_time, time_source, discovered_at, device_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record_id,
                object_id,
                file.source_type,
                file.account_id,
                file.conversation_hint,
                file.file_name,
                file.modified_time.to_rfc3339(),
                "mtime",
                Utc::now().to_rfc3339(),
                device_id
            ],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        // 3.3 插入或替换 local_files
        tx.execute(
            "INSERT INTO local_files (
                record_id, original_path, cache_path, size, mtime_ms, availability
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record_id,
                abs_path,
                Some(cache_path.to_string_lossy().to_string()),
                file.file_size as i64,
                mtime_ms,
                "available"
            ],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        // 3.4 插入 FTS5 全文索引
        tx.execute(
            "INSERT INTO file_search_fts (record_id, original_name) VALUES (?1, ?2)",
            params![record_id, file.file_name],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        // 3.5 创建待上传任务
        let task_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO upload_tasks (
                task_id, record_id, object_id, status, retry_count, error_message, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 0, NULL, ?5)",
            params![
                task_id,
                record_id,
                object_id,
                "queued",
                Utc::now().to_rfc3339()
            ],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        // 3.6 生成本机元数据日志事件（供同步发布）
        let event_id = Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "object_id": object_id,
            "record_id": record_id,
            "hash": hex_hash,
            "size": file.file_size,
            "mime": mime,
            "extension": extension,
            "object_created_at": Utc::now().to_rfc3339(),
            "source": file.source_type,
            "account_id": file.account_id,
            "conversation_id": file.conversation_hint,
            "original_name": file.file_name,
            "file_time": file.modified_time.to_rfc3339(),
            "time_source": "mtime",
            "discovered_at": Utc::now().to_rfc3339(),
            "device_id": device_id,
        });
        tx.execute(
            "INSERT INTO journal_events
             (event_id, device_id, epoch, seq, logical_clock, schema_version, event_type, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, 'file_record_added', ?6, ?7)",
            params![
                event_id,
                device_id,
                epoch as i64,
                next_seq as i64,
                logical_clock as i64,
                payload.to_string(),
                Utc::now().to_rfc3339()
            ],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        tx.commit()
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        Ok(IngestResult::Indexed {
            object_id,
            record_id,
            is_new_object,
        })
    }
}
