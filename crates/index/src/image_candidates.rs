// ChatVault 图片候选持久化：记录源文件状态、处理进度与错误码；不保存密钥或 code。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::{ImageCandidateStatus, ImageErrorCode};
use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

/// 图片候选记录
#[derive(Debug, Clone)]
pub struct ImageCandidateRow {
    pub candidate_id: String,
    pub source_root: String,
    pub source_account_id: String,
    pub source_path: String,
    pub source_size: i64,
    pub source_mtime_ms: i64,
    pub source_digest: Option<String>,
    pub conv_hash: Option<String>,
    pub month: Option<String>,
    pub normalized_stem: Option<String>,
    pub image_group_key: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
    pub attempt_count: i64,
    pub record_id: Option<String>,
}

type ExistingCandidate = (String, i64, i64, String, Option<String>, Option<String>);

#[allow(clippy::too_many_arguments)]
impl Database {
    /// 插入或更新图片候选（按 source_account_id + source_path 唯一）
    pub fn upsert_image_candidate(
        &mut self,
        source_root: &str,
        source_account_id: &str,
        source_path: &str,
        source_size: i64,
        source_mtime_ms: i64,
        conv_hash: Option<&str>,
        month: Option<&str>,
        normalized_stem: Option<&str>,
        image_group_key: Option<&str>,
    ) -> Result<String> {
        let now = Utc::now().to_rfc3339();
        let current_digest =
            chatvault_metadata::compute_blake3_file(std::path::Path::new(source_path))
                .ok()
                .map(|digest| digest.hex_hash);
        // 已存在则更新源状态并重置失败态（源可能已变化）
        let existing: Option<ExistingCandidate> = self
            .conn
            .query_row(
                "SELECT candidate_id, source_size, source_mtime_ms, status, source_digest, record_id FROM image_candidates
                 WHERE source_account_id=?1 AND source_path=?2",
                params![source_account_id, source_path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .ok();

        if let Some((id, old_size, old_mtime, _old_status, old_digest, record_id)) = existing {
            // 摘要缺失也视为变化：这样旧候选第一次重新扫描时会清掉残留状态。
            let digest_changed = old_digest.as_deref() != current_digest.as_deref();
            // 缓存缺失才触发重新准备：路径为空，或目标不是普通文件。
            let cache_missing = record_id.as_deref().is_some_and(|record_id| {
                self.conn
                    .query_row(
                        "SELECT cache_path FROM local_files WHERE record_id=?1",
                        params![record_id],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .ok()
                    .flatten()
                    .is_none_or(|path| {
                        !std::fs::symlink_metadata(&path)
                            .map(|metadata| {
                                metadata.file_type().is_file() && !metadata.file_type().is_symlink()
                            })
                            .unwrap_or(false)
                    })
            });
            let changed = old_size != source_size
                || old_mtime != source_mtime_ms
                || digest_changed
                || cache_missing;
            self.conn
                .execute(
                    "UPDATE image_candidates SET
                       source_size=?1, source_mtime_ms=?2,
                       conv_hash=COALESCE(?3, conv_hash),
                       month=COALESCE(?4, month),
                       normalized_stem=COALESCE(?5, normalized_stem),
                       image_group_key=COALESCE(?6, image_group_key),
                       status=CASE WHEN ?8 THEN 'discovered' ELSE status END,
                       error_code=CASE WHEN ?8 THEN NULL ELSE error_code END,
                       source_digest=CASE WHEN ?8 THEN ?7 ELSE COALESCE(?7, source_digest) END,
                       record_id=CASE WHEN ?8 THEN NULL ELSE record_id END,
                       attempt_count=CASE WHEN ?8 THEN 0 ELSE attempt_count END,
                       next_retry_ms=CASE WHEN ?8 THEN NULL ELSE next_retry_ms END,
                       updated_at=?9
                     WHERE candidate_id=?10",
                    params![
                        source_size,
                        source_mtime_ms,
                        conv_hash,
                        month,
                        normalized_stem,
                        image_group_key,
                        current_digest,
                        changed,
                        now,
                        id
                    ],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            return Ok(id);
        }

        let id = Uuid::new_v4().to_string();
        self.conn
            .execute(
                "INSERT INTO image_candidates (
                   candidate_id, source_root, source_account_id, source_path,
                   source_size, source_mtime_ms, source_digest, conv_hash, month,
                   normalized_stem, image_group_key, status, attempt_count,
                   created_at, updated_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'discovered',0,?12,?12)",
                params![
                    id,
                    source_root,
                    source_account_id,
                    source_path,
                    source_size,
                    source_mtime_ms,
                    current_digest,
                    conv_hash,
                    month,
                    normalized_stem,
                    image_group_key,
                    now
                ],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(id)
    }

    /// 标记候选失败
    pub fn mark_candidate_failed(
        &mut self,
        candidate_id: &str,
        error_code: ImageErrorCode,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let next_retry_ms = if error_code.auto_retry() {
            Some(Utc::now().timestamp_millis() + 3_600_000)
        } else {
            None
        };
        self.conn
            .execute(
                "UPDATE image_candidates SET
                   status='failed', error_code=?1, next_retry_ms=?2,
                   attempt_count=attempt_count+1, updated_at=?3
                 WHERE candidate_id=?4",
                params![error_code.as_str(), next_retry_ms, now, candidate_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 标记候选进入准备中
    pub fn mark_candidate_preparing(&mut self, candidate_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE image_candidates SET status='preparing', updated_at=?1 WHERE candidate_id=?2",
                params![now, candidate_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 标记候选明文已校验，等待入库
    pub fn mark_candidate_verified(&mut self, candidate_id: &str, digest: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE image_candidates SET
                   status='verified_plaintext', source_digest=?1, updated_at=?2
                 WHERE candidate_id=?3",
                params![digest, now, candidate_id],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 获取待处理候选（discovered / waiting_stable / failed 且允许重试）
    pub fn list_pending_image_candidates(
        &self,
        source_account_id: &str,
        now_ms: i64,
    ) -> Result<Vec<ImageCandidateRow>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT candidate_id, source_root, source_account_id, source_path,
                        source_size, source_mtime_ms, source_digest, conv_hash, month,
                        normalized_stem, image_group_key, status, error_code,
                        attempt_count, record_id
                 FROM image_candidates
                 WHERE source_account_id=?1
                   AND (
                     status IN ('discovered','waiting_stable','decrypted_pending_verify','verified_plaintext')
                     OR (status='failed' AND error_code IN (
                       'waiting_stable','empty_source','source_changed',
                       'media_parameters_unavailable','parameters_not_applicable',
                       'candidate_limit_exceeded','account_parameters_miss',
                       'insufficient_space','prepared_content_missing'
                     ))
                   )
                   AND (next_retry_ms IS NULL OR next_retry_ms <= ?2)
                 ORDER BY rowid",
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![source_account_id, now_ms], |row| {
                Ok(ImageCandidateRow {
                    candidate_id: row.get(0)?,
                    source_root: row.get(1)?,
                    source_account_id: row.get(2)?,
                    source_path: row.get(3)?,
                    source_size: row.get(4)?,
                    source_mtime_ms: row.get(5)?,
                    source_digest: row.get(6)?,
                    conv_hash: row.get(7)?,
                    month: row.get(8)?,
                    normalized_stem: row.get(9)?,
                    image_group_key: row.get(10)?,
                    status: row.get(11)?,
                    error_code: row.get(12)?,
                    attempt_count: row.get(13)?,
                    record_id: row.get(14)?,
                })
            })
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| ChatVaultError::Database(e.to_string()))?);
        }
        Ok(out)
    }

    /// 统计候选状态分布
    pub fn count_image_candidates_by_status(
        &self,
        source_account_id: Option<&str>,
    ) -> Result<Vec<(String, i64)>> {
        let sql = if source_account_id.is_some() {
            "SELECT status, COUNT(*) FROM image_candidates WHERE source_account_id=?1 GROUP BY status"
        } else {
            "SELECT status, COUNT(*) FROM image_candidates GROUP BY status"
        };
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let map = |row: &rusqlite::Row| -> rusqlite::Result<(String, i64)> {
            Ok((row.get(0)?, row.get(1)?))
        };
        let rows = if let Some(account) = source_account_id {
            stmt.query_map(params![account], map)
                .map_err(|e| ChatVaultError::Database(e.to_string()))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| ChatVaultError::Database(e.to_string()))?
        } else {
            stmt.query_map([], map)
                .map_err(|e| ChatVaultError::Database(e.to_string()))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| ChatVaultError::Database(e.to_string()))?
        };
        Ok(rows)
    }

    /// 显式重试：清除失败候选的错误码并重置为 discovered
    pub fn reset_failed_image_candidates(
        &mut self,
        source_account_id: Option<&str>,
    ) -> Result<usize> {
        let now = Utc::now().to_rfc3339();
        let count = if let Some(account) = source_account_id {
            self.conn
                .execute(
                    "UPDATE image_candidates SET
                       status='discovered', error_code=NULL, next_retry_ms=NULL, updated_at=?1
                     WHERE status='failed' AND source_account_id=?2",
                    params![now, account],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?
        } else {
            self.conn
                .execute(
                    "UPDATE image_candidates SET
                       status='discovered', error_code=NULL, next_retry_ms=NULL, updated_at=?1
                     WHERE status='failed'",
                    params![now],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?
        };
        Ok(count)
    }

    /// 将崩溃中断留下的准备状态重新排队；已入库候选不会被回退。
    pub fn recover_image_candidates(&mut self, source_account_id: Option<&str>) -> Result<usize> {
        let now = Utc::now().to_rfc3339();
        let sql = if source_account_id.is_some() {
            "UPDATE image_candidates SET status='discovered', error_code=NULL, next_retry_ms=NULL, updated_at=?1
             WHERE source_account_id=?2 AND status IN ('preparing','decrypted_pending_verify','verified_plaintext') AND record_id IS NULL"
        } else {
            "UPDATE image_candidates SET status='discovered', error_code=NULL, next_retry_ms=NULL, updated_at=?1
             WHERE status IN ('preparing','decrypted_pending_verify','verified_plaintext') AND record_id IS NULL"
        };
        let count = if let Some(account) = source_account_id {
            self.conn
                .execute(sql, params![now, account])
                .map_err(|e| ChatVaultError::Database(e.to_string()))?
        } else {
            self.conn
                .execute(sql, params![now])
                .map_err(|e| ChatVaultError::Database(e.to_string()))?
        };
        Ok(count)
    }

    /// 获取候选状态字符串（供状态机检查）
    pub fn get_candidate_status(&self, candidate_id: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT status FROM image_candidates WHERE candidate_id=?1",
                params![candidate_id],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))
    }
}

/// 将状态字符串解析为枚举（未知值视为 Discovered）
pub fn parse_candidate_status(s: &str) -> ImageCandidateStatus {
    match s {
        "waiting_stable" => ImageCandidateStatus::WaitingStable,
        "preparing" => ImageCandidateStatus::Preparing,
        "decrypted_pending_verify" => ImageCandidateStatus::DecryptedPendingVerify,
        "verified_plaintext" => ImageCandidateStatus::VerifiedPlaintext,
        "ingested" => ImageCandidateStatus::Ingested,
        "archived" => ImageCandidateStatus::Archived,
        "failed" => ImageCandidateStatus::Failed,
        _ => ImageCandidateStatus::Discovered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn upsert_and_list_candidates() {
        let mut db = temp_db();
        let id = db
            .upsert_image_candidate(
                r"C:\x\msg\attach",
                "wxid_a",
                r"C:\x\msg\attach\conv1\2026-09\Img\a.dat",
                100,
                1_700_000_000_000,
                Some("conv1"),
                Some("2026-09"),
                Some("a"),
                Some("gk1"),
            )
            .unwrap();
        assert!(!id.is_empty());

        let pending = db
            .list_pending_image_candidates("wxid_a", 9_999_999_999_999)
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].conv_hash.as_deref(), Some("conv1"));

        db.mark_candidate_failed(&id, ImageErrorCode::UnsupportedStructure)
            .unwrap();
        let stats = db.count_image_candidates_by_status(Some("wxid_a")).unwrap();
        assert_eq!(stats.iter().find(|(s, _)| s == "failed").unwrap().1, 1);

        let n = db.reset_failed_image_candidates(Some("wxid_a")).unwrap();
        assert_eq!(n, 1);
        let status = db.get_candidate_status(&id).unwrap().unwrap();
        assert_eq!(status, "discovered");
    }

    #[test]
    fn upsert_is_idempotent_on_same_path() {
        let mut db = temp_db();
        let a = db
            .upsert_image_candidate("root", "acc", "/p/a.dat", 1, 1, None, None, None, None)
            .unwrap();
        let b = db
            .upsert_image_candidate("root", "acc", "/p/a.dat", 2, 2, None, None, None, None)
            .unwrap();
        assert_eq!(a, b);
        let pending = db
            .list_pending_image_candidates("acc", 9_999_999_999_999)
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].source_size, 2);
    }

    #[test]
    fn upsert_keeps_status_when_cache_still_present() {
        let mut db = temp_db();
        let object_id = "blake3:cache-present-object";
        let now = Utc::now().to_rfc3339();
        let cache =
            std::env::temp_dir().join(format!("cv-candidate-cache-present-{}.bin", Uuid::new_v4()));
        std::fs::write(&cache, b"cached").unwrap();
        db.connection()
            .execute(
                "INSERT INTO file_objects (object_id,hash,size,mime,extension,created_at)
                 VALUES (?1,'cache-present-object',6,'image/png','png',?2)",
                rusqlite::params![object_id, now],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO file_records
                 (record_id,object_id,source_type,original_name,file_time,time_source,discovered_at,device_id)
                 VALUES ('record-cache-present',?1,'wechat-windows-4','a.png',?2,'mtime',?2,'device')",
                rusqlite::params![object_id, now],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO local_files
                 (record_id,original_path,cache_path,size,mtime_ms,availability,content_origin)
                 VALUES ('record-cache-present',?1,?2,6,1,'available','decrypted')",
                rusqlite::params![
                    r"C:\x\a.dat".to_string(),
                    cache.to_string_lossy().to_string()
                ],
            )
            .unwrap();

        let source =
            std::env::temp_dir().join(format!("cv-candidate-src-present-{}.dat", Uuid::new_v4()));
        std::fs::write(&source, b"aa").unwrap();
        let id = db
            .upsert_image_candidate(
                "root",
                "acc",
                source.to_str().unwrap(),
                2,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap();
        db.connection()
            .execute(
                "UPDATE image_candidates SET status='ingested', record_id='record-cache-present' WHERE candidate_id=?1",
                rusqlite::params![id],
            )
            .unwrap();

        // 源 size/mtime/摘要未变且缓存仍在，不得重置为 discovered
        db.upsert_image_candidate(
            "root",
            "acc",
            source.to_str().unwrap(),
            2,
            1,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            db.get_candidate_status(&id).unwrap().as_deref(),
            Some("ingested")
        );

        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_file(cache);
    }

    #[test]
    fn upsert_resets_when_cache_file_missing() {
        let mut db = temp_db();
        let object_id = "blake3:cache-missing-object";
        let now = Utc::now().to_rfc3339();
        let missing_cache =
            std::env::temp_dir().join(format!("cv-candidate-cache-missing-{}.bin", Uuid::new_v4()));
        db.connection()
            .execute(
                "INSERT INTO file_objects (object_id,hash,size,mime,extension,created_at)
                 VALUES (?1,'cache-missing-object',6,'image/png','png',?2)",
                rusqlite::params![object_id, now],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO file_records
                 (record_id,object_id,source_type,original_name,file_time,time_source,discovered_at,device_id)
                 VALUES ('record-cache-missing',?1,'wechat-windows-4','a.png',?2,'mtime',?2,'device')",
                rusqlite::params![object_id, now],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO local_files
                 (record_id,original_path,cache_path,size,mtime_ms,availability,content_origin)
                 VALUES ('record-cache-missing',?1,?2,6,1,'available','decrypted')",
                rusqlite::params![
                    r"C:\x\a.dat".to_string(),
                    missing_cache.to_string_lossy().to_string()
                ],
            )
            .unwrap();

        let source =
            std::env::temp_dir().join(format!("cv-candidate-src-missing-{}.dat", Uuid::new_v4()));
        std::fs::write(&source, b"aa").unwrap();
        let id = db
            .upsert_image_candidate(
                "root",
                "acc",
                source.to_str().unwrap(),
                2,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap();
        db.connection()
            .execute(
                "UPDATE image_candidates SET status='ingested', record_id='record-cache-missing' WHERE candidate_id=?1",
                rusqlite::params![id],
            )
            .unwrap();

        // 缓存文件不存在：即使源未变也应重置以便重新准备
        db.upsert_image_candidate(
            "root",
            "acc",
            source.to_str().unwrap(),
            2,
            1,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            db.get_candidate_status(&id).unwrap().as_deref(),
            Some("discovered")
        );

        let _ = std::fs::remove_file(source);
    }

    #[test]
    fn upsert_detects_same_size_overwrite_by_digest() {
        let mut db = temp_db();
        let path = std::env::temp_dir().join(format!("cv-candidate-digest-{}.dat", Uuid::new_v4()));
        std::fs::write(&path, b"aa").unwrap();
        let id = db
            .upsert_image_candidate(
                "root",
                "acc",
                path.to_str().unwrap(),
                2,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap();
        db.mark_candidate_failed(&id, ImageErrorCode::InvalidImage)
            .unwrap();
        std::fs::write(&path, b"bb").unwrap();
        db.upsert_image_candidate(
            "root",
            "acc",
            path.to_str().unwrap(),
            2,
            1,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            db.get_candidate_status(&id).unwrap().as_deref(),
            Some("discovered")
        );
        let _ = std::fs::remove_file(path);
    }
}
