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
        // 已存在则更新源状态并重置失败态（源可能已变化）
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT candidate_id FROM image_candidates
                 WHERE source_account_id=?1 AND source_path=?2",
                params![source_account_id, source_path],
                |r| r.get(0),
            )
            .ok();

        if let Some(id) = existing {
            self.conn
                .execute(
                    "UPDATE image_candidates SET
                       source_size=?1, source_mtime_ms=?2,
                       conv_hash=COALESCE(?3, conv_hash),
                       month=COALESCE(?4, month),
                       normalized_stem=COALESCE(?5, normalized_stem),
                       image_group_key=COALESCE(?6, image_group_key),
                       updated_at=?7
                     WHERE candidate_id=?8",
                    params![
                        source_size,
                        source_mtime_ms,
                        conv_hash,
                        month,
                        normalized_stem,
                        image_group_key,
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
                   source_size, source_mtime_ms, conv_hash, month,
                   normalized_stem, image_group_key, status, attempt_count,
                   created_at, updated_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'discovered',0,?11,?11)",
                params![
                    id,
                    source_root,
                    source_account_id,
                    source_path,
                    source_size,
                    source_mtime_ms,
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
        self.conn
            .execute(
                "UPDATE image_candidates SET
                   status='failed', error_code=?1,
                   attempt_count=attempt_count+1, updated_at=?2
                 WHERE candidate_id=?3",
                params![error_code.as_str(), now, candidate_id],
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
                   AND status IN ('discovered','waiting_stable','failed','decrypted_pending_verify')
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
}
