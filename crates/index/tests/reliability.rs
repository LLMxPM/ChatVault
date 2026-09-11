// ChatVault 索引回归测试：历史副本、旧库升级、身份、分页过滤与删除状态。
use chatvault_core::models::{DiscoveredFile, JournalEvent, JournalEventType};
use chatvault_index::{Database, IngestResult, SearchFilter, SearchService};
use chrono::Utc;

/// 从测试文件构造发现元数据；入库仍自行读取并验证实际文件状态。
fn discovered(path: &std::path::Path) -> DiscoveredFile {
    DiscoveredFile {
        source_type: "test".into(),
        account_id: None,
        absolute_path: path.to_string_lossy().into(),
        file_name: path.file_name().unwrap().to_string_lossy().into(),
        file_size: 0,
        modified_time: Utc::now(),
        conversation_hint: None,
    }
}

/// 同路径更新不会删除旧版本副本；源文件消失后两项任务都能取得原内容。
#[test]
fn historical_versions_survive_source_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"old").unwrap();
    let mut db = Database::open(dir.path().join("index.db")).unwrap();
    db.ingest_file(&discovered(&path), "a").unwrap();
    assert!(matches!(
        db.ingest_file(&discovered(&path), "a").unwrap(),
        IngestResult::Skipped { .. }
    ));
    std::fs::write(&path, b"new version").unwrap();
    db.ingest_file(&discovered(&path), "a").unwrap();
    std::fs::remove_file(&path).unwrap();
    let tasks = db.pending_uploads().unwrap();
    assert_eq!(tasks.len(), 2);
    let mut contents: Vec<_> = tasks
        .iter()
        .map(|t| std::fs::read(db.upload_source(&t.task_id).unwrap()).unwrap())
        .collect();
    contents.sort();
    assert_eq!(contents, vec![b"new version".to_vec(), b"old".to_vec()]);
    drop(db);
    let mut reopened = Database::open(dir.path().join("index.db")).unwrap();
    assert!(reopened
        .upload_source(&tasks[0].task_id)
        .unwrap()
        .is_absolute());
}

/// 用户选择包含索引和暂存的父目录时，不会递归归档应用自身文件。
#[test]
fn application_storage_is_excluded_from_ingestion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"file").unwrap();
    let index = dir.path().join("index.db");
    let mut db = Database::open(&index).unwrap();
    db.ingest_file(&discovered(&path), "a").unwrap();
    let id = db.pending_uploads().unwrap()[0].task_id.clone();
    let cached = db.upload_source(&id).unwrap();
    assert!(matches!(
        db.ingest_file(&discovered(&cached), "a").unwrap(),
        IngestResult::Skipped { .. }
    ));
    assert!(matches!(
        db.ingest_file(&discovered(&index), "a").unwrap(),
        IngestResult::Skipped { .. }
    ));
    assert_eq!(db.get_stats().unwrap().total_records, 1);
}

/// 可重试任务遵循退避，但用户手动重试可以立即重新入队。
#[test]
fn retries_and_pause_use_persistent_queue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"file").unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.ingest_file(&discovered(&path), "a").unwrap();
    let id = db.pending_uploads().unwrap()[0].task_id.clone();
    db.update_task_status(&id, "retryable_failed", Some("offline"))
        .unwrap();
    assert!(db.pending_uploads().unwrap().is_empty());
    db.connection()
        .execute(
            "UPDATE upload_tasks SET updated_at='2020-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    assert_eq!(db.pending_uploads().unwrap().len(), 1);
    db.pause_task(&id).unwrap();
    assert!(!db.task_is_runnable(&id).unwrap());
    db.requeue_task(&id).unwrap();
    assert_eq!(db.pending_uploads().unwrap().len(), 1);
    assert!(db
        .list_upload_tasks(Some("queued' OR 1=1 --"), 100)
        .unwrap()
        .is_empty());
}

/// 两个新数据库生成不同身份，同一个索引重新打开后身份不变。
#[test]
fn identity_is_unique_and_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.db");
    let mut a = Database::open(&path).unwrap();
    let id = a.ensure_device_identity().unwrap();
    drop(a);
    assert_eq!(
        Database::open(&path)
            .unwrap()
            .ensure_device_identity()
            .unwrap(),
        id
    );
    assert_ne!(
        Database::open_in_memory()
            .unwrap()
            .ensure_device_identity()
            .unwrap(),
        id
    );
}

/// 旧固定身份只迁移一次；来源 ID 不变，新事件使用唯一设备身份重新发布。
#[test]
fn legacy_identity_reissues_events_once() {
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting("device_id", "win-pc-01").unwrap();
    let mut old = event(1, "旧文件.pdf");
    old.device_id = "win-pc-01".into();
    old.seq = 1;
    db.insert_journal_event_if_absent(&old).unwrap();
    let id = db.ensure_device_identity().unwrap();
    assert_ne!(id, "win-pc-01");
    assert_eq!(db.ensure_device_identity().unwrap(), id);
    let reissued = db.list_unpublished_journal_events(&id, 0, 10).unwrap();
    assert_eq!(reissued.len(), 1);
    assert_ne!(reissued[0].event_id, old.event_id);
    assert_eq!(reissued[0].payload["record_id"], old.payload["record_id"]);
}

/// 已绑定远端只接受同一地址和 Vault，防止复用旧同步游标。
#[test]
fn binding_rejects_switches() {
    let mut db = Database::open_in_memory().unwrap();
    db.bind_remote("https://example.test/dav/", "a").unwrap();
    db.bind_remote("https://example.test/dav", "a").unwrap();
    assert!(db.bind_remote("https://example.test/dav", "b").is_err());
    assert!(db.bind_remote("https://other.test/dav", "a").is_err());
    assert!(db
        .check_remote_binding("https://example.test/dav", "../a")
        .is_err());
}

/// 先过滤再分页；晚于 500 条图片的文档仍能查询到。
#[test]
fn category_filter_precedes_pagination() {
    let mut db = Database::open_in_memory().unwrap();
    for seq in 0..502 {
        let ev = event(
            seq,
            if seq < 501 {
                "图片.png"
            } else {
                "旧文档.pdf"
            },
        );
        db.apply_file_record_added_event(&ev).unwrap();
    }
    let search = SearchService::new(&db);
    let docs = search
        .search(&SearchFilter {
            category: Some("doc".into()),
            limit: 100,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].original_name, "旧文档.pdf");
    let first = search
        .search(&SearchFilter {
            category: Some("image".into()),
            limit: 100,
            ..Default::default()
        })
        .unwrap();
    let second = search
        .search(&SearchFilter {
            category: Some("image".into()),
            limit: 100,
            offset: 100,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(first.len(), 100);
    assert_eq!(second.len(), 100);
    assert!(!first
        .iter()
        .any(|a| second.iter().any(|b| a.record_id == b.record_id)));
    assert!(search
        .search(&SearchFilter {
            keyword: Some("a\"b".into()),
            ..Default::default()
        })
        .is_ok());
}

/// 删除优先于后到达的新增事件，所有搜索长度都隐藏同一记录。
#[test]
fn tombstone_survives_out_of_order_addition() {
    let mut db = Database::open_in_memory().unwrap();
    let added = event(1, "中文测试文件.pdf");
    let mut deleted = added.clone();
    deleted.event_type = JournalEventType::FileRecordDeleted;
    deleted.event_id = "deleted".into();
    db.apply_file_record_deleted_event(&deleted).unwrap();
    db.apply_file_record_added_event(&added).unwrap();
    for keyword in [
        None,
        Some("中".into()),
        Some("中文".into()),
        Some("中文测试".into()),
    ] {
        assert!(SearchService::new(&db)
            .search(&SearchFilter {
                keyword,
                ..Default::default()
            })
            .unwrap()
            .is_empty());
    }
}

/// 将等价旧版结构迁移两次，原记录和待上传任务保留，旧来源可补建副本。
#[test]
fn legacy_schema_migration_preserves_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file.txt");
    std::fs::write(&file, b"legacy").unwrap();
    let path = dir.path().join("index.db");
    let mut db = Database::open(&path).unwrap();
    db.ingest_file(&discovered(&file), "a").unwrap();
    db.connection().execute_batch("UPDATE local_files SET cache_path=NULL; CREATE UNIQUE INDEX old_unique_path ON local_files(original_path); DROP TABLE record_tombstones; PRAGMA user_version=0;").unwrap();
    drop(db);
    let mut upgraded = Database::open(&path).unwrap();
    assert!(matches!(
        upgraded.ingest_file(&discovered(&file), "a").unwrap(),
        IngestResult::Skipped { .. }
    ));
    assert_eq!(upgraded.get_stats().unwrap().total_records, 1);
    let id = upgraded.pending_uploads().unwrap()[0].task_id.clone();
    assert_eq!(
        std::fs::read(upgraded.upload_source(&id).unwrap()).unwrap(),
        b"legacy"
    );
    drop(upgraded);
    assert_eq!(
        Database::open(&path)
            .unwrap()
            .get_stats()
            .unwrap()
            .total_records,
        1
    );
}

/// 构造数据库层来源事件，数据全部来自测试常量。
fn event(seq: u64, name: &str) -> JournalEvent {
    JournalEvent {
        event_id: format!("e{seq}"),
        device_id: "a".into(),
        epoch: 1,
        seq: seq + 1,
        logical_clock: seq + 1,
        schema_version: 1,
        event_type: JournalEventType::FileRecordAdded,
        created_at: Utc::now(),
        payload: serde_json::json!({
            "object_id":"blake3:test","hash":"test","size":7,"record_id":format!("r{seq}"),"original_name":name,
            "file_time":if seq<501 {"2026-09-11T00:00:00Z"}else{"2020-01-01T00:00:00Z"}}),
    }
}
