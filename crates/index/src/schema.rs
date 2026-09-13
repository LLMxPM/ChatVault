//! # 本地数据库结构定义与初始化
//!
//! 定义 ChatVault 的 SQLite 关系表结构、FTS5 全文检索引擎虚表与索引。
//! 包含内容去重表、文件来源记录表、本机文件映射表、全文检索表和上传任务表。

use chatvault_core::error::{ChatVaultError, Result};
use rusqlite::Connection;

/// 直接初始化当前本地数据库架构
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
            width INTEGER,
            height INTEGER,
            frame_count INTEGER,
            created_at TEXT NOT NULL
        );

        -- 2. 文件来源记录表（多次接收同内容生成多条来源）
        CREATE TABLE IF NOT EXISTS file_records (
            record_id TEXT PRIMARY KEY,
            object_id TEXT NOT NULL REFERENCES file_objects(object_id) ON DELETE CASCADE,
            source_type TEXT NOT NULL,
            source_account_id TEXT,
            source_conversation_id TEXT,
            original_name TEXT NOT NULL,
            file_time TEXT NOT NULL,
            time_source TEXT NOT NULL,
            discovered_at TEXT NOT NULL,
            device_id TEXT NOT NULL,
            media_variant TEXT,
            image_group_key TEXT,
            source_original_name TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_records_object_id ON file_records(object_id);
        CREATE INDEX IF NOT EXISTS idx_records_source_account_id ON file_records(source_type, source_account_id);
        CREATE INDEX IF NOT EXISTS idx_records_source_conversation_id ON file_records(source_type, source_account_id, source_conversation_id);
        CREATE INDEX IF NOT EXISTS idx_records_file_time ON file_records(file_time);
        CREATE INDEX IF NOT EXISTS idx_records_image_group ON file_records(image_group_key);

        -- 3. 来源账号映射：原始 ID 是稳定匹配键，用户只维护 display_name 与收藏状态
        CREATE TABLE IF NOT EXISTS source_accounts (
            source_type TEXT NOT NULL,
            source_account_id TEXT NOT NULL,
            source_name TEXT,
            display_name TEXT,
            is_favorite INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            updated_logical_clock INTEGER NOT NULL DEFAULT 0,
            updated_device_id TEXT NOT NULL DEFAULT '',
            updated_event_id TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (source_type, source_account_id)
        );
        CREATE INDEX IF NOT EXISTS idx_source_accounts_favorite ON source_accounts(is_favorite, source_type, source_account_id);

        -- 4. 来源聊天映射：按来源类型、账号和聊天 ID 联合隔离
        CREATE TABLE IF NOT EXISTS source_conversations (
            source_type TEXT NOT NULL,
            source_account_id TEXT NOT NULL,
            source_conversation_id TEXT NOT NULL,
            source_name TEXT,
            display_name TEXT,
            is_favorite INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            updated_logical_clock INTEGER NOT NULL DEFAULT 0,
            updated_device_id TEXT NOT NULL DEFAULT '',
            updated_event_id TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (source_type, source_account_id, source_conversation_id)
        );
        CREATE INDEX IF NOT EXISTS idx_source_conversations_favorite ON source_conversations(is_favorite, source_type, source_account_id, source_conversation_id);

        -- 5. 本机文件路径与缓存映射表
        CREATE TABLE IF NOT EXISTS local_files (
            record_id TEXT PRIMARY KEY REFERENCES file_records(record_id) ON DELETE CASCADE,
            original_path TEXT NOT NULL,
            cache_path TEXT,
            size INTEGER NOT NULL,
            source_size INTEGER,
            mtime_ms INTEGER NOT NULL,
            availability TEXT NOT NULL,
            content_origin TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_local_path ON local_files(original_path);

        -- 6. FTS5 中文全文检索虚表 (使用 trigram 分词器)
        CREATE VIRTUAL TABLE IF NOT EXISTS file_search_fts USING fts5(
            record_id UNINDEXED,
            original_name,
            tokenize='trigram'
        );

        -- 7. 上传归档任务表
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

        -- 8. 应用设置键值表（Vault/设备/WebDAV/定时/采集目录）
        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- 9. 本机已生成的元数据日志事件
        CREATE TABLE IF NOT EXISTS journal_events (
            event_id TEXT PRIMARY KEY,
            device_id TEXT NOT NULL,
            epoch INTEGER NOT NULL,
            seq INTEGER NOT NULL,
            logical_clock INTEGER NOT NULL,
            schema_version INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL,
            UNIQUE(device_id, epoch, seq)
        );

        -- 10. 已应用的远端事件（幂等重放）
        CREATE TABLE IF NOT EXISTS applied_events (
            event_id TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL
        );

        -- 11. 各设备同步游标
        CREATE TABLE IF NOT EXISTS sync_cursors (
            device_id TEXT NOT NULL,
            epoch INTEGER NOT NULL,
            last_contiguous_seq INTEGER NOT NULL,
            PRIMARY KEY(device_id, epoch)
        );

        -- 删除标记支持删除事件先于新增事件到达
        CREATE TABLE IF NOT EXISTS record_tombstones (
            record_id TEXT PRIMARY KEY,
            event_id TEXT NOT NULL
        );

        -- 12. 远端已知设备注册表
        CREATE TABLE IF NOT EXISTS known_devices (
            device_id TEXT PRIMARY KEY,
            display_name TEXT,
            epoch INTEGER NOT NULL,
            last_seq INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- 13. 扫描根检查点：last_scan_started_ms 为本次开始时刻，避免漏扫扫描期间新建文件
        CREATE TABLE IF NOT EXISTS scan_roots (
            root_path TEXT PRIMARY KEY,
            source_kind TEXT NOT NULL,
            source_account_id TEXT,
            last_scan_started_ms INTEGER NOT NULL
        );

        -- 14. 图片候选持久化：采集根、账号与规范化来源路径标识候选；不保存密钥、code
        CREATE TABLE IF NOT EXISTS image_candidates (
            candidate_id TEXT PRIMARY KEY,
            source_root TEXT NOT NULL,
            source_account_id TEXT NOT NULL,
            source_path TEXT NOT NULL,
            source_size INTEGER NOT NULL,
            source_mtime_ms INTEGER NOT NULL,
            source_digest TEXT,
            conv_hash TEXT,
            month TEXT,
            normalized_stem TEXT,
            image_group_key TEXT,
            status TEXT NOT NULL,
            error_code TEXT,
            attempt_count INTEGER NOT NULL DEFAULT 0,
            next_retry_ms INTEGER,
            record_id TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(source_account_id, source_path)
        );
        CREATE INDEX IF NOT EXISTS idx_image_candidates_status ON image_candidates(status, next_retry_ms);
        CREATE INDEX IF NOT EXISTS idx_image_candidates_group ON image_candidates(image_group_key);

        -- 15. 运行实体：一次流水线/恢复
        CREATE TABLE IF NOT EXISTS task_runs (
            run_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            trigger_source TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            duration_ms INTEGER,
            webdav_configured INTEGER NOT NULL DEFAULT 0,
            summary_json TEXT,
            error_message TEXT,
            runner_kind TEXT NOT NULL DEFAULT 'desktop',
            runner_pid INTEGER,
            cancel_requested INTEGER NOT NULL DEFAULT 0,
            heartbeat_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_task_runs_started ON task_runs(started_at DESC);
        CREATE INDEX IF NOT EXISTS idx_task_runs_status ON task_runs(status);

        -- 16. 运行内阶段：scan → archive → publish → pull
        CREATE TABLE IF NOT EXISTS task_run_stages (
            stage_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
            stage TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT,
            finished_at TEXT,
            duration_ms INTEGER,
            stats_json TEXT,
            message TEXT,
            UNIQUE(run_id, stage)
        );

        -- 17. 运行文件级关键明细（失败/跳过/缺失等）
        CREATE TABLE IF NOT EXISTS task_run_items (
            item_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
            stage TEXT NOT NULL,
            record_id TEXT,
            object_id TEXT,
            task_id TEXT,
            name TEXT NOT NULL,
            status TEXT NOT NULL,
            error_code TEXT,
            error_message TEXT,
            size INTEGER,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_task_run_items_run ON task_run_items(run_id, stage, status);
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
            .prepare(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='file_objects'",
            )
            .unwrap();
        let count: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
    }
}
