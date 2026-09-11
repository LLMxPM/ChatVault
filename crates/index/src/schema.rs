//! # 本地数据库结构定义与初始化
//!
//! 定义 ChatVault 的 SQLite 关系表结构、FTS5 全文检索引擎虚表与索引。
//! 包含内容去重表、文件来源记录表、本机文件映射表、全文检索表和上传任务表。

use chatvault_core::error::{ChatVaultError, Result};
use rusqlite::Connection;

/// 初始化或迁移本地数据库架构
///
/// 职责: 创建核心表与 FTS5 trigram 全文索引，启用 WAL 日志模式
/// 输入: `conn`: SQLite 数据库连接实例
/// 输出: `Result<()>`
/// 关键约束:
///   - 必须使用支持 FTS5 的 SQLite 编译选项
///   - 采用 trigram 分词器支持中文任意子串高效查询
pub fn initialize_schema(conn: &Connection) -> Result<()> {
    // 启用 WAL 模式提高并发读写性能
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| ChatVaultError::Database(format!("设置 WAL 模式失败: {}", e)))?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| ChatVaultError::Database(format!("设置 synchronous 失败: {}", e)))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| ChatVaultError::Database(format!("开启外键约束失败: {}", e)))?;

    conn.execute_batch(
        r#"
        -- 1. 内容寻址对象表（以哈希为主键，全局去重）
        CREATE TABLE IF NOT EXISTS file_objects (
            object_id TEXT PRIMARY KEY,
            hash TEXT NOT NULL UNIQUE,
            size INTEGER NOT NULL,
            mime TEXT NOT NULL,
            extension TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        -- 2. 文件来源记录表（多次接收同内容生成多条来源）
        CREATE TABLE IF NOT EXISTS file_records (
            record_id TEXT PRIMARY KEY,
            object_id TEXT NOT NULL REFERENCES file_objects(object_id) ON DELETE CASCADE,
            source TEXT NOT NULL,
            account_id TEXT,
            conversation_id TEXT,
            original_name TEXT NOT NULL,
            file_time TEXT NOT NULL,
            time_source TEXT NOT NULL,
            discovered_at TEXT NOT NULL,
            device_id TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_records_object_id ON file_records(object_id);
        CREATE INDEX IF NOT EXISTS idx_records_account_id ON file_records(account_id);
        CREATE INDEX IF NOT EXISTS idx_records_file_time ON file_records(file_time);

        -- 3. 本机文件路径与缓存映射表
        CREATE TABLE IF NOT EXISTS local_files (
            record_id TEXT PRIMARY KEY REFERENCES file_records(record_id) ON DELETE CASCADE,
            original_path TEXT NOT NULL UNIQUE,
            cache_path TEXT,
            size INTEGER NOT NULL,
            mtime_ms INTEGER NOT NULL,
            availability TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_local_path ON local_files(original_path);

        -- 4. FTS5 中文全文检索虚表 (使用 trigram 分词器)
        CREATE VIRTUAL TABLE IF NOT EXISTS file_search_fts USING fts5(
            record_id UNINDEXED,
            original_name,
            tokenize='trigram'
        );

        -- 5. 上传归档任务表
        CREATE TABLE IF NOT EXISTS upload_tasks (
            task_id TEXT PRIMARY KEY,
            record_id TEXT NOT NULL REFERENCES file_records(record_id) ON DELETE CASCADE,
            object_id TEXT NOT NULL REFERENCES file_objects(object_id) ON DELETE CASCADE,
            status TEXT NOT NULL,
            retry_count INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tasks_status ON upload_tasks(status);
        "#,
    )
    .map_err(|e| ChatVaultError::Database(format!("执行表结构初始化失败: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_init() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(initialize_schema(&conn).is_ok());

        // 验证表是否存在
        let mut stmt = conn
            .prepare("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='file_objects'")
            .unwrap();
        let count: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
    }
}
