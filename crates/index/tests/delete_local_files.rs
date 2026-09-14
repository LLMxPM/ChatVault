// ChatVault 删除本机原文件安全边界回归：备份门禁、采集源白名单与受控缓存边界。
use chatvault_core::models::{CollectSource, DiscoveredFile, GENERIC_FOLDER_SOURCE_TYPE};
use chatvault_index::cache_policy::COPY_THRESHOLD_MIB;
use chatvault_index::Database;
use std::path::{Path, PathBuf};

/// 构造扫描输入。
fn discovered(path: &Path) -> DiscoveredFile {
    DiscoveredFile {
        source_type: GENERIC_FOLDER_SOURCE_TYPE.into(),
        source_account_id: None,
        absolute_path: path.to_string_lossy().into(),
        file_name: path.file_name().unwrap().to_string_lossy().into(),
        file_size: 0,
        modified_time: chrono::Utc::now(),
        source_conversation_id: None,
    }
}

/// 写入附件并入库，返回 (原文件路径, object_id)。
fn ingest(db: &mut Database, directory: &Path, name: &str, bytes: &[u8]) -> (PathBuf, String) {
    let path = directory.join(name);
    std::fs::write(&path, bytes).unwrap();
    let result = db.ingest_file(&discovered(&path), "a").unwrap();
    let object_id = match result {
        chatvault_index::IngestResult::Indexed { object_id, .. } => object_id,
        _ => panic!("ingest should index a new file"),
    };
    (path, object_id)
}

/// 标记全部上传完成，并模拟元数据已发布。
fn complete(db: &mut Database) {
    for task in db.pending_uploads().unwrap() {
        db.update_task_status(&task.task_id, "backed_up", None)
            .unwrap();
    }
    let seq = db.next_journal_seq("a").unwrap() - 1;
    db.upsert_sync_cursor("a", 1, seq).unwrap();
}

/// 将采集目录登记为 generic 源。
fn allow_source(db: &mut Database, root: &Path) {
    let sources = vec![CollectSource {
        source_type: GENERIC_FOLDER_SOURCE_TYPE.into(),
        path: root.to_string_lossy().into_owned(),
        enable_videos: true,
    }];
    db.set_setting("collect_sources", &serde_json::to_string(&sources).unwrap())
        .unwrap();
}

/// 未完成归档时必须拒绝删除原文件。
#[test]
fn refuse_delete_before_backup() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(COPY_THRESHOLD_MIB, "1").unwrap();
    allow_source(&mut db, dir.path());
    let (path, object_id) = ingest(&mut db, dir.path(), "pending.bin", b"unique-pending");
    let err = db.delete_object_local_files(&object_id).unwrap_err();
    assert!(err.to_string().contains("尚未完成远端归档"));
    assert!(path.exists());
    let count: i64 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM local_files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

/// 已 backed_up 但元数据未发布时仍拒绝。
#[test]
fn refuse_delete_before_journal_publish() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(COPY_THRESHOLD_MIB, "1").unwrap();
    allow_source(&mut db, dir.path());
    let (path, object_id) = ingest(&mut db, dir.path(), "unpub.bin", b"unique-unpub");
    for task in db.pending_uploads().unwrap() {
        db.update_task_status(&task.task_id, "backed_up", None)
            .unwrap();
    }
    let err = db.delete_object_local_files(&object_id).unwrap_err();
    assert!(err.to_string().contains("尚未完成远端归档"));
    assert!(path.exists());
}

/// 有采集源配置时，路径不在根下拒绝删除。
#[test]
fn refuse_delete_outside_collect_roots() {
    let source_dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(COPY_THRESHOLD_MIB, "1").unwrap();
    allow_source(&mut db, source_dir.path());
    // 从「其它目录」入库，路径不在已登记采集源内。
    let (path, object_id) = ingest(&mut db, other_dir.path(), "outside.bin", b"outside");
    complete(&mut db);
    let err = db.delete_object_local_files(&object_id).unwrap_err();
    assert!(err.to_string().contains("不在已配置采集目录内"));
    assert!(path.exists());
}

/// 归档完成且路径在采集源内时允许删除原文件与受控副本。
#[test]
fn delete_original_when_archived_and_under_source() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(COPY_THRESHOLD_MIB, "1").unwrap();
    allow_source(&mut db, dir.path());
    let (path, object_id) = ingest(&mut db, dir.path(), "ok.bin", b"delete-me");
    let task = db.pending_uploads().unwrap().remove(0);
    let cache = db.upload_source(&task.task_id).unwrap();
    complete(&mut db);
    assert!(cache.exists());
    let (origins, caches, released) = db.delete_object_local_files(&object_id).unwrap();
    assert_eq!(origins, 1);
    assert!(caches >= 1);
    assert!(released >= 8);
    assert!(!path.exists());
    assert!(!cache.exists());
    let count: i64 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM local_files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

/// 映射缓存路径被篡改到受控目录外时拒绝删除。
#[test]
fn refuse_delete_when_cache_path_escapes_staging() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(COPY_THRESHOLD_MIB, "1").unwrap();
    allow_source(&mut db, dir.path());
    let (_path, object_id) = ingest(&mut db, dir.path(), "esc.bin", b"escape-cache");
    complete(&mut db);
    let staging = db.staging_dir();
    db.connection()
        .execute(
            "UPDATE local_files SET cache_path = ?1",
            [dir.path().join("esc.bin").to_string_lossy()],
        )
        .unwrap();
    let _ = staging;
    let err = db.delete_object_local_files(&object_id).unwrap_err();
    assert!(err.to_string().contains("缓存路径不在受控目录内"));
}

/// 未配置采集源时仍执行备份门禁，完成归档后可删。
#[test]
fn allow_delete_without_collect_sources_after_backup() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting(COPY_THRESHOLD_MIB, "1").unwrap();
    let (path, object_id) = ingest(&mut db, dir.path(), "nosrc.bin", b"no-source-root");
    complete(&mut db);
    let (origins, _, _) = db.delete_object_local_files(&object_id).unwrap();
    assert_eq!(origins, 1);
    assert!(!path.exists());
}
