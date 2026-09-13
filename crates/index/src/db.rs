//! # 本地数据库访问与增量入库管理
//!
//! 提供 SQLite 数据库连接封装、增量比对入库、同内容对象去重、
//! 来源记录追加以及数据概览统计功能。

use crate::schema::initialize_schema;
use chatvault_core::error::{ChatVaultError, Result};
#[cfg(test)]
use chatvault_core::models::DiscoveredFile;

#[cfg(test)]
use chrono::Utc;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

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
    pub(crate) conn: Connection,
    db_path: Option<PathBuf>,
    pub(crate) staging_dir: PathBuf,
    _temporary: Option<tempfile::TempDir>,
}

impl Database {
    /// 打开或创建指定文件路径的 SQLite 数据库
    ///
    /// 职责: 初始化数据库连接并创建当前表结构
    /// 输入: `path`: 数据库文件绝对或相对路径
    /// 输出: `Result<Self>`
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(p).map_err(|e| {
            ChatVaultError::Database(format!("打开数据库失败 {}: {}", p.display(), e))
        })?;

        initialize_schema(&conn)?;
        let p = std::fs::canonicalize(p)?;

        Ok(Self {
            conn,
            db_path: Some(p.to_path_buf()),
            staging_dir: p.with_extension("objects"),
            _temporary: None,
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

        let temporary = tempfile::tempdir()?;
        Ok(Self {
            staging_dir: temporary.path().to_path_buf(),
            _temporary: Some(temporary),
            conn,
            db_path: None,
        })
    }

    /// 进程启动时的运行历史维护：异常退出的 running 标记失败，并清理过期记录。
    ///
    /// 必须只在应用/CLI 启动时调用一次。禁止在 `open`/`get_db` 中调用——
    /// 桌面端每次命令都会新开连接，若在此清理会把进行中的流水线误标为失败。
    pub fn maintain_task_runs_on_startup(&mut self) -> Result<()> {
        let stale = self.mark_stale_running_task_runs()?;
        if stale > 0 {
            tracing::warn!("发现 {stale} 个未正常结束的任务运行，已标记为失败");
        }
        let purged = self.purge_old_task_runs(50, 30)?;
        if purged > 0 {
            tracing::info!("清理过期任务运行记录 {purged} 条");
        }
        Ok(())
    }

    /// 获取数据库底层路径（内存数据库为 None）
    pub fn path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }

    /// 获取受控暂存目录（明文对象与解密产物的存放位置）
    pub fn staging_dir(&self) -> PathBuf {
        self.staging_dir.clone()
    }

    /// 获取底层只读/可变连接引用
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
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
            .query_row("SELECT count(*) FROM file_records r WHERE NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id=r.record_id)", [], |r| r.get(0))
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

/// 上传任务行
#[derive(Debug, Clone)]
pub struct UploadTaskRow {
    pub task_id: String,
    pub status: String,
    pub retry_count: u32,
    pub error_message: Option<String>,
    pub updated_at: String,
    pub original_name: String,
    pub record_id: String,
    pub hash: String,
    pub size: u64,
    pub original_path: Option<String>,
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
            source_account_id: Some("user1".to_string()),
            absolute_path: file1.to_str().unwrap().to_string(),
            file_name: "test_ingest_1.txt".to_string(),
            file_size: 22,
            modified_time: Utc::now(),
            source_conversation_id: None,
        };

        let df2 = DiscoveredFile {
            source_type: "test".to_string(),
            source_account_id: Some("user2".to_string()),
            absolute_path: file2.to_str().unwrap().to_string(),
            file_name: "test_ingest_2.txt".to_string(),
            file_size: 22,
            modified_time: Utc::now(),
            source_conversation_id: None,
        };

        // 第一次入库
        let res1 = db.ingest_file(&df1, "pc-01").unwrap();
        assert!(matches!(
            res1,
            IngestResult::Indexed {
                is_new_object: true,
                ..
            }
        ));

        // 重复扫描同一个路径，验证幂等跳过
        let res1_repeat = db.ingest_file(&df1, "pc-01").unwrap();
        assert!(matches!(res1_repeat, IngestResult::Skipped { .. }));

        // 第二个不同路径但内容相同的文件入库，验证去重复用 Object
        let res2 = db.ingest_file(&df2, "pc-01").unwrap();
        assert!(matches!(
            res2,
            IngestResult::Indexed {
                is_new_object: false,
                ..
            }
        ));

        let stats = db.get_stats().unwrap();
        assert_eq!(stats.total_objects, 1); // 内容去重为 1
        assert_eq!(stats.total_records, 2); // 来源记录为 2
        assert_eq!(db.list_source_accounts().unwrap().len(), 2); // 来源账号按联合键去重

        let _ = std::fs::remove_file(file1);
        let _ = std::fs::remove_file(file2);
    }
}
