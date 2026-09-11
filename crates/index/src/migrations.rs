// ChatVault 索引升级：保留历史版本位置与删除标记，兼容旧版数据库。
use chatvault_core::error::{ChatVaultError, Result};
use rusqlite::Connection;

/// 在单个事务中升级旧表；重复打开不会重建或删除用户数据。
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE").map_err(db_error)?;
    let result = (|| {
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(db_error)?;
        if version < 1 {
            conn.execute_batch(r#"
                CREATE TABLE local_files_v2 (
                    record_id TEXT PRIMARY KEY REFERENCES file_records(record_id) ON DELETE CASCADE,
                    original_path TEXT NOT NULL, cache_path TEXT, size INTEGER NOT NULL,
                    mtime_ms INTEGER NOT NULL, availability TEXT NOT NULL
                );
                INSERT INTO local_files_v2 SELECT * FROM local_files;
                DROP TABLE local_files;
                ALTER TABLE local_files_v2 RENAME TO local_files;
                CREATE INDEX idx_local_path ON local_files(original_path);
                CREATE TABLE record_tombstones (record_id TEXT PRIMARY KEY, event_id TEXT NOT NULL);
                INSERT OR IGNORE INTO record_tombstones
                    SELECT json_extract(payload, '$.record_id'), event_id FROM journal_events
                    WHERE event_type = 'file_record_deleted' AND json_extract(payload, '$.record_id') IS NOT NULL;
                PRAGMA user_version = 1;
            "#).map_err(db_error)?;
        }
        if version < 2 {
            conn.execute_batch(
                r#"
                CREATE TABLE sync_cursors_v2 (
                    device_id TEXT NOT NULL, epoch INTEGER NOT NULL,
                    last_contiguous_seq INTEGER NOT NULL, PRIMARY KEY(device_id,epoch)
                );
                INSERT INTO sync_cursors_v2 SELECT * FROM sync_cursors;
                DROP TABLE sync_cursors;
                ALTER TABLE sync_cursors_v2 RENAME TO sync_cursors;
                PRAGMA user_version=2;
            "#,
            )
            .map_err(db_error)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT").map_err(db_error),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// 将 SQLite 错误转换为领域错误。
fn db_error(e: rusqlite::Error) -> ChatVaultError {
    ChatVaultError::Database(e.to_string())
}
