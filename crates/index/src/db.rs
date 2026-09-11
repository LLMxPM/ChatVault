//! # 本地数据库访问与增量入库管理
//!
//! 提供 SQLite 数据库连接封装、增量比对入库、同内容对象去重、
//! 来源记录追加以及数据概览统计功能。

use crate::schema::initialize_schema;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::DiscoveredFile;
use chatvault_metadata::hasher::compute_blake3_file;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// 入库操作返回结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestResult {
    /// 文件已存在且内容与时间未发生变化，跳过处理
    Skipped { path: String },
    /// 新增或更新了记录
    Indexed {
        object_id: String,
        record_id: String,
        /// 是否是系统中此前未见过的新内容对象
        is_new_object: bool,
    },
}

/// 数据库概览统计
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DatabaseStats {
    /// 唯一内容对象数量 (去重后)
    pub total_objects: usize,
    /// 文件来源记录总数
    pub total_records: usize,
    /// 本机可用文件数量
    pub local_files: usize,
    /// 待归档上传任务数
    pub pending_tasks: usize,
}

/// 本地索引数据库管理器
pub struct Database {
    conn: Connection,
    db_path: Option<PathBuf>,
}

impl Database {
    /// 打开或创建指定文件路径的 SQLite 数据库
    ///
    /// 职责: 初始化数据库连接并自动迁移表结构
    /// 输入: `path`: 数据库文件绝对或相对路径
    /// 输出: `Result<Self>`
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(p)
            .map_err(|e| ChatVaultError::Database(format!("打开数据库失败 {}: {}", p.display(), e)))?;

        initialize_schema(&conn)?;

        Ok(Self {
            conn,
            db_path: Some(p.to_path_buf()),
        })
    }

    /// 创建纯内存临时数据库（主要用于单元测试与快速演练）
    ///
    /// 职责: 创建与初始化内存数据库
    /// 输出: `Result<Self>`
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ChatVaultError::Database(format!("创建内存数据库失败: {}", e)))?;

        initialize_schema(&conn)?;

        Ok(Self {
            conn,
            db_path: None,
        })
    }

    /// 获取数据库底层路径（内存数据库为 None）
    pub fn path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }

    /// 获取底层只读/可变连接引用
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

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
        let abs_path = &file.absolute_path;
        let mtime_ms = file.modified_time.timestamp_millis();

        // 1. 检查本机文件表，若路径和 mtime 完全一致则判定为已入库，跳过
        let existing_mtime: Option<i64> = {
            let mut check_stmt = self
                .conn
                .prepare("SELECT mtime_ms FROM local_files WHERE original_path = ?1")
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            check_stmt
                .query_row(params![abs_path], |row| row.get(0))
                .ok()
        };

        if let Some(prev_mtime) = existing_mtime {
            if prev_mtime == mtime_ms {
                return Ok(IngestResult::Skipped {
                    path: abs_path.clone(),
                });
            }
        }

        // 2. 文件是新的或 mtime 发生了变化，计算 BLAKE3 哈希
        let hash_res = compute_blake3_file(abs_path)?;
        let object_id = hash_res.object_id;
        let hex_hash = hash_res.hex_hash;
        let extension = Path::new(abs_path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let mime = format!("application/{}", extension); // 基础推断

        // 3. 在事务中写入 SQLite
        let tx = self
            .conn
            .transaction()
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

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
            "INSERT OR REPLACE INTO local_files (
                record_id, original_path, cache_path, size, mtime_ms, availability
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record_id,
                abs_path,
                None::<String>,
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
            params![task_id, record_id, object_id, "queued", Utc::now().to_rfc3339()],
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

    /// 获取当前资料库统计信息
    ///
    /// 职责: 统计内容对象数、来源记录数、本机文件数和待上传任务数
    /// 输出: `Result<DatabaseStats>`
    pub fn get_stats(&self) -> Result<DatabaseStats> {
        let count_objs: i64 = self
            .conn
            .query_row("SELECT count(*) FROM file_objects", [], |r| r.get(0))
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let count_records: i64 = self
            .conn
            .query_row("SELECT count(*) FROM file_records", [], |r| r.get(0))
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let count_local: i64 = self
            .conn
            .query_row("SELECT count(*) FROM local_files", [], |r| r.get(0))
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let count_pending: i64 = self
            .conn
            .query_row(
                "SELECT count(*) FROM upload_tasks WHERE status = 'queued'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        Ok(DatabaseStats {
            total_objects: count_objs as usize,
            total_records: count_records as usize,
            local_files: count_local as usize,
            pending_tasks: count_pending as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_ingest_and_deduplicate() {
        let mut db = Database::open_in_memory().unwrap();
        let temp_dir = std::env::temp_dir();
        let file1 = temp_dir.join("test_ingest_1.txt");
        let file2 = temp_dir.join("test_ingest_2.txt");

        let mut f1 = File::create(&file1).unwrap();
        f1.write_all(b"same content for dedup").unwrap();
        drop(f1);

        let mut f2 = File::create(&file2).unwrap();
        f2.write_all(b"same content for dedup").unwrap();
        drop(f2);

        let df1 = DiscoveredFile {
            source_type: "test".to_string(),
            account_id: Some("user1".to_string()),
            absolute_path: file1.to_str().unwrap().to_string(),
            file_name: "test_ingest_1.txt".to_string(),
            file_size: 22,
            modified_time: Utc::now(),
            conversation_hint: None,
        };

        let df2 = DiscoveredFile {
            source_type: "test".to_string(),
            account_id: Some("user2".to_string()),
            absolute_path: file2.to_str().unwrap().to_string(),
            file_name: "test_ingest_2.txt".to_string(),
            file_size: 22,
            modified_time: Utc::now(),
            conversation_hint: None,
        };

        // 第一次入库
        let res1 = db.ingest_file(&df1, "pc-01").unwrap();
        assert!(matches!(res1, IngestResult::Indexed { is_new_object: true, .. }));

        // 重复扫描同一个路径，验证幂等跳过
        let res1_repeat = db.ingest_file(&df1, "pc-01").unwrap();
        assert!(matches!(res1_repeat, IngestResult::Skipped { .. }));

        // 第二个不同路径但内容相同的文件入库，验证去重复用 Object
        let res2 = db.ingest_file(&df2, "pc-01").unwrap();
        assert!(matches!(res2, IngestResult::Indexed { is_new_object: false, .. }));

        let stats = db.get_stats().unwrap();
        assert_eq!(stats.total_objects, 1); // 内容去重为 1
        assert_eq!(stats.total_records, 2); // 来源记录为 2

        let _ = std::fs::remove_file(file1);
        let _ = std::fs::remove_file(file2);
    }
}
