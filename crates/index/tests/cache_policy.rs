// ChatVault 缓存策略回归：阈值边界、原文件变化、共享引用和归档后回收。
use chatvault_core::models::DiscoveredFile;
use chatvault_index::{cache_policy::*, Database, IngestResult};
use std::path::{Path, PathBuf};

/// 构造扫描输入，真实大小和时间由入库读取。
fn discovered(path: &Path) -> DiscoveredFile {
    DiscoveredFile {
        source_type: "test".into(),
        account_id: None,
        absolute_path: path.to_string_lossy().into(),
        file_name: path.file_name().unwrap().to_string_lossy().into(),
        file_size: 0,
        modified_time: chrono::Utc::now(),
        conversation_hint: None,
    }
}

/// 写入附件并入库，返回原文件路径。
fn ingest(db: &mut Database, directory: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, bytes).unwrap();
    db.ingest_file(&discovered(&path), "a").unwrap();
    path
}

/// 标记全部上传完成，并模拟已发布元数据游标。
fn complete(db: &mut Database) {
    for task in db.pending_uploads().unwrap() {
        db.update_task_status(&task.task_id, "backed_up", None)
            .unwrap();
    }
    let seq = db.next_journal_seq("a").unwrap() - 1;
    db.upsert_sync_cursor("a", 1, seq).unwrap();
}

/// 小于阈值复制，等于与超过阈值不复制；重复扫描不生成新任务。
#[test]
fn threshold_boundary_and_zero() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(COPY_THRESHOLD_MIB, "1").unwrap();
    for size in [MIB - 1, MIB, MIB + 1] {
        let path = ingest(
            &mut db,
            dir.path(),
            &size.to_string(),
            &vec![0; size as usize],
        );
        let cache: Option<String> = db
            .connection()
            .query_row(
                "SELECT cache_path FROM local_files ORDER BY rowid DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cache.is_some(), size < MIB);
        assert!(matches!(
            db.ingest_file(&discovered(&path), "a").unwrap(),
            IngestResult::Skipped { .. }
        ));
    }
    db.set_setting(COPY_THRESHOLD_MIB, "0").unwrap();
    ingest(&mut db, dir.path(), "empty", b"");
    let count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM local_files WHERE cache_path IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(db.pending_uploads().unwrap().len(), 4);
}

/// 直接上传不允许原文件内容发生变化，也不允许原文件丢失。
#[test]
fn direct_source_must_match_ingested_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(COPY_THRESHOLD_MIB, "0").unwrap();
    let path = ingest(&mut db, dir.path(), "source", b"old");
    let task = db.pending_uploads().unwrap().remove(0);
    assert_eq!(
        std::fs::read(db.upload_source(&task.task_id).unwrap()).unwrap(),
        b"old"
    );
    std::fs::write(&path, b"new").unwrap();
    assert!(matches!(
        db.upload_source(&task.task_id),
        Err(chatvault_core::error::ChatVaultError::HashMismatch { .. })
    ));
    std::fs::remove_file(path).unwrap();
    assert!(db.upload_source(&task.task_id).is_err());
}

/// 元数据未同步和共享副本仍有待处理引用时，容量为零也不能删除。
#[test]
fn protect_pending_and_unpublished_then_reclaim_without_recopy() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(CACHE_MAX_MIB, "0").unwrap();
    let path = ingest(&mut db, dir.path(), "one", b"shared");
    ingest(&mut db, dir.path(), "two", b"shared");
    let tasks = db.pending_uploads().unwrap();
    let cache = db.upload_source(&tasks[0].task_id).unwrap();
    assert_eq!(cache, db.upload_source(&tasks[1].task_id).unwrap());
    assert_eq!(db.reclaim_cache().unwrap(), 0);
    db.update_task_status(&tasks[0].task_id, "backed_up", None)
        .unwrap();
    db.upsert_sync_cursor("a", 1, 2).unwrap();
    for state in ["queued", "retryable_failed", "paused", "missing"] {
        db.update_task_status(&tasks[1].task_id, state, None)
            .unwrap();
        assert_eq!(db.reclaim_cache().unwrap(), 0);
    }
    db.update_task_status(&tasks[1].task_id, "backed_up", None)
        .unwrap();
    db.connection()
        .execute("DELETE FROM sync_cursors", [])
        .unwrap();
    assert_eq!(db.reclaim_cache().unwrap(), 0);
    db.upsert_sync_cursor("a", 1, 2).unwrap();
    assert_eq!(db.reclaim_cache().unwrap(), 6);
    assert!(!cache.exists());
    assert_eq!(std::fs::read(&path).unwrap(), b"shared");
    assert!(matches!(
        db.ingest_file(&discovered(&path), "a").unwrap(),
        IngestResult::Skipped { .. }
    ));
    assert!(!cache.exists());
    assert_eq!(db.reclaim_cache().unwrap(), 0);
}

/// 超量时按归档完成时间回收，未到期且满足容量的缓存继续保留。
#[test]
fn quota_evicts_oldest_and_expiry_evicts_remaining() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(CACHE_MAX_MIB, "1").unwrap();
    ingest(&mut db, dir.path(), "older", &vec![1; 700_000]);
    let older = db.pending_uploads().unwrap().remove(0);
    let old_cache = db.upload_source(&older.task_id).unwrap();
    ingest(&mut db, dir.path(), "newer", &vec![2; 700_000]);
    complete(&mut db);
    db.connection()
        .execute(
            "UPDATE upload_tasks SET updated_at=datetime('now','-1 day') WHERE task_id=?1",
            [&older.task_id],
        )
        .unwrap();
    assert_eq!(db.reclaim_cache().unwrap(), 700_000);
    assert!(!old_cache.exists());
    assert_eq!(db.reclaim_cache().unwrap(), 0);
    db.connection()
        .execute(
            "UPDATE upload_tasks SET updated_at=datetime('now','-8 days')",
            [],
        )
        .unwrap();
    assert_eq!(db.reclaim_cache().unwrap(), 700_000);
}

/// 只清理受控目录的无引用哈希对象，未知文件和原附件不受影响。
#[test]
fn orphan_cleanup_keeps_unknown_files_and_sources() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(CACHE_RETENTION_DAYS, "0").unwrap();
    let source = ingest(&mut db, dir.path(), "source", b"orphan");
    let task = db.pending_uploads().unwrap().remove(0);
    let cache = db.upload_source(&task.task_id).unwrap();
    let unknown = cache.parent().unwrap().join("keep.tmp");
    std::fs::write(&unknown, b"keep").unwrap();
    db.connection()
        .execute("DELETE FROM file_records", [])
        .unwrap();
    assert_eq!(db.reclaim_cache().unwrap(), 6);
    assert!(!cache.exists());
    assert!(unknown.exists());
    assert!(source.exists());
}

/// 策略解析必须拒绝负数，且配置能跨进程重新打开后保留。
#[test]
fn settings_persist_and_reject_invalid_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.db");
    let mut db = Database::open(&path).unwrap();
    db.set_setting(COPY_THRESHOLD_MIB, "25").unwrap();
    drop(db);
    let mut db = Database::open(path).unwrap();
    assert_eq!(db.cache_policy().unwrap().copy_threshold_mib, 25);
    db.set_setting(CACHE_MAX_MIB, "-1").unwrap();
    assert!(db.cache_policy().is_err());
}
