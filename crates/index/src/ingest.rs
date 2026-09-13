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
    ///     输出: `Result<IngestResult>`
    ///     关键约束:
    ///   - 整个入库过程在 SQLite 事务中执行，保证原子性
    ///   - 相同内容复用已有的 file_objects，仅追加 file_records
    pub fn ingest_file(&mut self, file: &DiscoveredFile, device_id: &str) -> Result<IngestResult> {
        let mut file = file.clone();
        let source = std::fs::canonicalize(&file.absolute_path)?;
        let is_index = self
            .path()
            .map(|db| {
                source == db
                    || source.to_string_lossy() == format!("{}-wal", db.display())
                    || source.to_string_lossy() == format!("{}-shm", db.display())
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
                && cache
                    .as_ref()
                    .is_none_or(|p| is_regular_file_path(Path::new(p)))
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
                "INSERT INTO file_objects
                 (object_id, hash, size, mime, extension, width, height, frame_count, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL, ?6)",
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
                record_id, original_path, cache_path, size, source_size, mtime_ms, availability, content_origin
             ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, 'original')",
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
        // 受控路径校验：准备产物必须位于 pending 目录的直接子项。
        let pending_dir = self.staging_dir.join("pending");
        if plaintext.parent() != Some(pending_dir.as_path()) {
            return Err(ChatVaultError::Internal(
                "prepared content 路径不在受控 pending 目录内".into(),
            ));
        }
        let plaintext_metadata =
            std::fs::symlink_metadata(plaintext).map_err(|_| ChatVaultError::FileNotFound {
                path: prepared.plaintext_path.clone(),
            })?;
        if plaintext_metadata.file_type().is_symlink() || !plaintext_metadata.file_type().is_file()
        {
            return Err(ChatVaultError::FileNotFound {
                path: prepared.plaintext_path.clone(),
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

        // 持有数据库写锁时再次复核 pending 明文哈希，避免入库期间被替换。
        let pending_metadata = std::fs::symlink_metadata(plaintext)?;
        if pending_metadata.file_type().is_symlink() || !pending_metadata.file_type().is_file() {
            return Err(ChatVaultError::FileNotFound {
                path: prepared.plaintext_path.clone(),
            });
        }
        let actual = chatvault_metadata::compute_blake3_file(plaintext)?;
        if actual.hex_hash != prepared.content_hash {
            // 校验失败的 pending 文件不可再用于重试，避免残缺文件长期占据工作目录。
            let _ = std::fs::remove_file(plaintext);
            return Err(ChatVaultError::HashMismatch {
                expected: prepared.content_hash.clone(),
                actual: actual.hex_hash,
            });
        }

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
        let indexed: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT l.record_id, l.cache_path FROM local_files l JOIN file_records r ON l.record_id=r.record_id
                 WHERE l.original_path=?1 AND r.object_id=?2 LIMIT 1",
                params![prepared.source_path, object_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        if let Some((indexed_record_id, indexed_cache)) = indexed {
            std::fs::create_dir_all(&self.staging_dir)?;
            let object_path = self.staging_dir.join(&prepared.content_hash);
            let indexed_cache_valid = indexed_cache.as_deref().is_some_and(|path| {
                let cache = std::path::Path::new(path);
                cache.is_file()
                    && chatvault_metadata::verify_file_hash(cache, &prepared.content_hash).is_ok()
            });
            if indexed_cache_valid {
                std::fs::remove_file(plaintext)?;
            } else {
                install_pending_object(plaintext, &object_path, &prepared.content_hash)?;
                tx.execute(
                    "UPDATE local_files SET cache_path=?1, availability='available' WHERE record_id=?2",
                    params![object_path.to_string_lossy().to_string(), indexed_record_id],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            }
            if let Some(cid) = candidate_id {
                tx.execute(
                    "UPDATE image_candidates SET status='ingested', record_id=?1, error_code=NULL, source_digest=?2, updated_at=?3 WHERE candidate_id=?4",
                    params![indexed_record_id, prepared.source_digest, Utc::now().to_rfc3339(), cid],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            }
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

        // 在同一 SQLite 写事务持有期间，将 pending 明文原子安装为最终对象。
        // 目标已存在时只校验并丢弃 pending 副本，防止残缺文件阻塞后续重试。
        std::fs::create_dir_all(&self.staging_dir)?;
        let object_path = self.staging_dir.join(&prepared.content_hash);
        install_pending_object(plaintext, &object_path, &prepared.content_hash)?;
        let cache_path = object_path.to_string_lossy().to_string();

        if is_new_object {
            tx.execute(
                "INSERT INTO file_objects
                 (object_id, hash, size, mime, extension, width, height, frame_count, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    object_id,
                    prepared.content_hash,
                    prepared.size as i64,
                    mime,
                    extension,
                    prepared.width,
                    prepared.height,
                    prepared.frame_count,
                    Utc::now().to_rfc3339()
                ],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        } else {
            tx.execute(
                "UPDATE file_objects SET width=COALESCE(width, ?1), height=COALESCE(height, ?2),
                 frame_count=COALESCE(frame_count, ?3) WHERE object_id=?4",
                params![
                    prepared.width,
                    prepared.height,
                    prepared.frame_count,
                    object_id
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
                record_id, original_path, cache_path, size, source_size, mtime_ms, availability, content_origin
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'decrypted')",
            params![
                record_id,
                prepared.source_path,
                cache_path,
                prepared.size as i64,
                prepared.source_size as i64,
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
                 error_code=NULL, source_digest=?4 WHERE candidate_id=?3",
                params![
                    record_id,
                    Utc::now().to_rfc3339(),
                    cid,
                    prepared.source_digest
                ],
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

/// 判断受控缓存是否为真实普通文件，避免把符号链接当作本地副本。
fn is_regular_file_path(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

/// 将已校验的 pending 文件安装为最终对象。
///
/// 目标不存在时使用 rename；目标存在且内容正确时复用目标并删除 pending；
/// 目标是残缺普通文件时先删除再安装，绝不把 pending 直接覆盖到未知文件上。
fn install_pending_object(pending: &Path, target: &Path, expected_hash: &str) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                return Err(ChatVaultError::Internal("对象缓存路径不是普通文件".into()));
            }
            if chatvault_metadata::verify_file_hash(target, expected_hash).is_ok() {
                std::fs::remove_file(pending)?;
                return Ok(());
            }
            // 目标是残缺对象，删除后再以原子 rename 安装完整 pending 文件。
            std::fs::remove_file(target)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    match std::fs::rename(pending, target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // 并发入库可能先完成；只接受已完整校验的同哈希对象。
            chatvault_metadata::verify_file_hash(target, expected_hash)?;
            std::fs::remove_file(pending)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_pending_file_is_removed_before_retry() {
        let mut db = Database::open_in_memory().unwrap();
        let pending_dir = db.staging_dir().join("pending");
        std::fs::create_dir_all(&pending_dir).unwrap();
        let pending = pending_dir.join("partial");
        std::fs::write(&pending, b"partial").unwrap();
        let expected_hash = chatvault_metadata::compute_blake3_bytes(b"complete").hex_hash;
        let prepared = PreparedContent {
            plaintext_path: pending.to_string_lossy().into_owned(),
            content_hash: expected_hash,
            size: 8,
            mime: "image/png".into(),
            extension: "png".into(),
            width: Some(1),
            height: Some(1),
            frame_count: Some(1),
            source_type: "wechat-windows-4".into(),
            source_account_id: Some("account".into()),
            source_conversation_id: Some("conversation".into()),
            export_name: "image.png".into(),
            source_original_name: "image.dat".into(),
            source_path: "C:\\wechat\\image.dat".into(),
            source_mtime_ms: 1,
            source_size: 7,
            source_digest: Some("source-digest".into()),
            media_variant: Some("display".into()),
            image_group_key: Some("group".into()),
            file_time: Utc::now(),
        };
        assert!(matches!(
            db.ingest_prepared_content(&prepared, "device", None),
            Err(ChatVaultError::HashMismatch { .. })
        ));
        assert!(!pending.exists());
    }
}
