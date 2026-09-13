// ChatVault 文件入库：受控暂存、增量发现、内容去重与本机事件原子写入。
use crate::db::*;
use crate::source_mappings::{ensure_source_account, ensure_source_conversation};
use chatvault_core::{
    error::{ChatVaultError, Result},
    models::{DiscoveredFile, PreparedContent},
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

        if let Some((prev_mtime, prev_size, cache)) = existing_mtime {
            if prev_mtime == mtime_ms
                && prev_size == file.file_size as i64
                && cache.as_ref().is_none_or(|p| Path::new(p).exists())
            {
                return Ok(IngestResult::Skipped {
                    path: abs_path.clone(),
                });
            }
        }

        let policy = self.cache_policy()?;
        let epoch = self
            .get_setting(&format!("epoch_{}", device_id))?
            .and_then(|v| v.parse().ok())
            .unwrap_or(1u64);
        // 复制与引用写入共用写锁，防止回收器删除刚落盘的副本。
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let (cache_path, hash_res, modified) =
            if file.file_size < u64::from(policy.copy_threshold_mib) * crate::cache_policy::MIB {
                let (path, hash, modified) =
                    chatvault_metadata::staging::stage_file(&source, &self.staging_dir)?;
                (Some(path.to_string_lossy().to_string()), hash, modified)
            } else {
                let (hash, modified) = chatvault_metadata::staging::hash_stable_file(&source)?;
                (None, hash, modified)
            };
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
        let mime = format!("application/{}", extension);
        let next_seq: i64 = tx.query_row("SELECT COALESCE(MAX(seq), 0)+1 FROM journal_events WHERE device_id=?1 AND epoch=?2", params![device_id, epoch], |r| r.get(0))
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let logical_clock: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(logical_clock), 0)+1 FROM journal_events",
                [],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let source_account_id = file
            .source_account_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        let source_conversation_id = source_account_id.as_ref().and_then(|_| {
            file.source_conversation_id
                .clone()
                .filter(|value| !value.trim().is_empty())
        });
        let mapping_now = Utc::now().to_rfc3339();
        if let Some(account_id) = source_account_id.as_deref() {
            // 扫描入库只创建映射占位，不改变已有的自定义名称和收藏状态。
            ensure_source_account(
                &tx,
                &file.source_type,
                account_id,
                Some(account_id),
                &mapping_now,
            )?;
            if let Some(conversation_id) = source_conversation_id.as_deref() {
                ensure_source_conversation(
                    &tx,
                    &file.source_type,
                    account_id,
                    conversation_id,
                    Some(conversation_id),
                    &mapping_now,
                )?;
            }
        }
        // 重新检查发现键，处理等待写锁期间另一个扫描进程已入库的情况。
        let indexed: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM local_files l JOIN file_records r ON l.record_id=r.record_id WHERE l.original_path=?1 AND l.mtime_ms=?2 AND l.size=?3 AND r.object_id=?4)",
            params![abs_path, mtime_ms, file.file_size as i64, object_id], |r| r.get(0))
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        if indexed {
            tx.execute("UPDATE local_files SET cache_path=?1 WHERE original_path=?2 AND mtime_ms=?3 AND record_id IN (SELECT record_id FROM file_records WHERE object_id=?4)",
                params![cache_path,abs_path,mtime_ms,object_id]).map_err(|e|ChatVaultError::Database(e.to_string()))?;
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
                record_id, object_id, source_type, source_account_id, source_conversation_id,
                original_name, file_time, time_source, discovered_at, device_id,
                media_variant, image_group_key, source_original_name
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL, NULL)",
            params![
                record_id,
                object_id,
                file.source_type,
                source_account_id,
                source_conversation_id,
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
                record_id, original_path, cache_path, size, mtime_ms, availability, content_origin
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'original')",
            params![
                record_id,
                abs_path,
                cache_path,
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
            "source_type": file.source_type,
            "source_account_id": source_account_id,
            "source_conversation_id": source_conversation_id,
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
                logical_clock,
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

    /// 事务入库已校验的明文内容（图片解密产物）
    ///
    /// 职责: 验证明文路径与哈希后，写入对象、来源、本机路径、上传任务与日志事件。
    /// 关键约束:
    ///   - 明文暂存路径必须位于受控 staging 目录内
    ///   - 密钥/code/参数目录文件名不得出现在任何字段
    ///   - 整个过程在 SQLite 事务中执行
    pub fn ingest_prepared_content(
        &mut self,
        prepared: &PreparedContent,
        device_id: &str,
        candidate_id: Option<&str>,
    ) -> Result<IngestResult> {
        let plaintext = Path::new(&prepared.plaintext_path);
        // 受控路径校验：必须位于 staging 目录内
        if !plaintext.starts_with(&self.staging_dir) {
            return Err(ChatVaultError::Internal(
                "prepared content 路径不在受控暂存目录内".into(),
            ));
        }
        if !plaintext.is_file() {
            return Err(ChatVaultError::FileNotFound {
                path: prepared.plaintext_path.clone(),
            });
        }
        // 复核哈希
        let actual = chatvault_metadata::compute_blake3_file(plaintext)?;
        if actual.hex_hash != prepared.content_hash {
            return Err(ChatVaultError::HashMismatch {
                expected: prepared.content_hash.clone(),
                actual: actual.hex_hash,
            });
        }

        let object_id = format!("blake3:{}", prepared.content_hash);
        let mime = prepared.mime.clone();
        let extension = prepared.extension.clone();

        let policy = self.cache_policy()?;
        let epoch = self
            .get_setting(&format!("epoch_{}", device_id))?
            .and_then(|v| v.parse().ok())
            .unwrap_or(1u64);

        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        // 明文已是受控对象，直接引用；copy_threshold 不影响保留
        let cache_path = prepared.plaintext_path.clone();

        let source_account_id = prepared
            .source_account_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        let source_conversation_id = source_account_id.as_ref().and_then(|_| {
            prepared
                .source_conversation_id
                .clone()
                .filter(|value| !value.trim().is_empty())
        });

        let mapping_now = Utc::now().to_rfc3339();
        if let Some(account_id) = source_account_id.as_deref() {
            ensure_source_account(
                &tx,
                &prepared.source_type,
                account_id,
                Some(account_id),
                &mapping_now,
            )?;
            if let Some(conversation_id) = source_conversation_id.as_deref() {
                ensure_source_conversation(
                    &tx,
                    &prepared.source_type,
                    account_id,
                    conversation_id,
                    Some(conversation_id),
                    &mapping_now,
                )?;
            }
        }

        // 幂等：同一源路径 + 同一对象已入库则跳过
        let indexed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM local_files l JOIN file_records r ON l.record_id=r.record_id
             WHERE l.original_path=?1 AND r.object_id=?2)",
            params![prepared.source_path, object_id],
            |r| r.get(0),
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        if indexed {
            tx.commit()
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            return Ok(IngestResult::Skipped {
                path: prepared.source_path.clone(),
            });
        }

        let is_new_object = {
            let count: i64 = tx
                .query_row(
                    "SELECT count(*) FROM file_objects WHERE object_id = ?1",
                    params![object_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            count == 0
        };

        if is_new_object {
            tx.execute(
                "INSERT INTO file_objects (object_id, hash, size, mime, extension, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    object_id,
                    prepared.content_hash,
                    prepared.size as i64,
                    mime,
                    extension,
                    Utc::now().to_rfc3339()
                ],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        }

        let record_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO file_records (
                record_id, object_id, source_type, source_account_id, source_conversation_id,
                original_name, file_time, time_source, discovered_at, device_id,
                media_variant, image_group_key, source_original_name
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record_id,
                object_id,
                prepared.source_type,
                source_account_id,
                source_conversation_id,
                prepared.export_name,
                prepared.file_time.to_rfc3339(),
                "mtime",
                Utc::now().to_rfc3339(),
                device_id,
                prepared.media_variant,
                prepared.image_group_key,
                prepared.source_original_name,
            ],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        // 本机映射：original_path 指向源 .dat，cache_path 指向明文
        // 打开/上传时优先使用 cache_path（明文）
        tx.execute(
            "INSERT INTO local_files (
                record_id, original_path, cache_path, size, mtime_ms, availability, content_origin
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'decrypted')",
            params![
                record_id,
                prepared.source_path,
                cache_path,
                prepared.size as i64,
                prepared.source_mtime_ms,
                "available"
            ],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        // FTS 索引使用导出名
        tx.execute(
            "INSERT INTO file_search_fts (record_id, original_name) VALUES (?1, ?2)",
            params![record_id, prepared.export_name],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        // 上传任务
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

        // 更新候选状态
        if let Some(cid) = candidate_id {
            tx.execute(
                "UPDATE image_candidates SET status='ingested', record_id=?1, updated_at=?2,
                 error_code=NULL WHERE candidate_id=?3",
                params![record_id, Utc::now().to_rfc3339(), cid],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        }

        // 日志事件
        let next_seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0)+1 FROM journal_events WHERE device_id=?1 AND epoch=?2",
            params![device_id, epoch],
            |r| r.get(0),
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let logical_clock: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(logical_clock), 0)+1 FROM journal_events",
                [],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let event_id = Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "object_id": object_id,
            "record_id": record_id,
            "hash": prepared.content_hash,
            "size": prepared.size,
            "mime": mime,
            "extension": extension,
            "object_created_at": Utc::now().to_rfc3339(),
            "source_type": prepared.source_type,
            "source_account_id": source_account_id,
            "source_conversation_id": source_conversation_id,
            "original_name": prepared.export_name,
            "source_original_name": prepared.source_original_name,
            "file_time": prepared.file_time.to_rfc3339(),
            "time_source": "mtime",
            "discovered_at": Utc::now().to_rfc3339(),
            "device_id": device_id,
            "media_variant": prepared.media_variant,
            "image_group_key": prepared.image_group_key,
            "width": prepared.width,
            "height": prepared.height,
            "frame_count": prepared.frame_count,
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
                logical_clock,
                payload.to_string(),
                Utc::now().to_rfc3339()
            ],
        )
        .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let _ = policy;
        tx.commit()
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        Ok(IngestResult::Indexed {
            object_id,
            record_id,
            is_new_object,
        })
    }
}
