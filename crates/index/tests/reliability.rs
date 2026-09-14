// ChatVault 索引回归测试：历史副本、身份、分页过滤与删除状态。
use chatvault_core::models::{DiscoveredFile, JournalEvent, JournalEventType};
use chatvault_index::{Database, IngestResult, SearchFilter, SearchService};
use chrono::Utc;

/// 从测试文件构造发现元数据；入库仍自行读取并验证实际文件状态。
fn discovered(path: &std::path::Path) -> DiscoveredFile {
    DiscoveredFile {
        source_type: "test".into(),
        source_account_id: None,
        absolute_path: path.to_string_lossy().into(),
        file_name: path.file_name().unwrap().to_string_lossy().into(),
        file_size: 0,
        modified_time: Utc::now(),
        source_conversation_id: None,
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
    let reopened = Database::open(dir.path().join("index.db")).unwrap();
    assert!(reopened
        .upload_source(&tasks[0].task_id)
        .unwrap()
        .is_absolute());
}

/// 原文件仍存在时，缺失或损坏的受控副本也必须报错，不能重新读取原路径。
#[test]
fn upload_requires_intact_staged_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"original").unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.ingest_file(&discovered(&path), "a").unwrap();
    let id = db.pending_uploads().unwrap()[0].task_id.clone();
    let cached = db.upload_source(&id).unwrap();
    std::fs::write(&cached, b"corrupted").unwrap();
    assert!(matches!(
        db.upload_source(&id),
        Err(chatvault_core::error::ChatVaultError::HashMismatch { .. })
    ));
    std::fs::remove_file(&cached).unwrap();
    assert!(matches!(
        db.upload_source(&id),
        Err(chatvault_core::error::ChatVaultError::FileNotFound { .. })
    ));
    assert!(!cached.exists());
    db.connection()
        .execute("UPDATE local_files SET cache_path=NULL", [])
        .unwrap();
    assert_eq!(
        std::fs::read(db.upload_source(&id).unwrap()).unwrap(),
        b"original"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
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
    // pending 虚拟筛选：含待处理状态，不含已校验
    db.pause_task(&id).unwrap();
    assert_eq!(db.list_upload_tasks(Some("pending"), 100).unwrap().len(), 1);
    db.update_task_status(&id, "backed_up", None).unwrap();
    assert!(db
        .list_upload_tasks(Some("pending"), 100)
        .unwrap()
        .is_empty());
    assert_eq!(
        db.list_upload_tasks(Some("backed_up"), 100).unwrap().len(),
        1
    );
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

/// 已绑定远端只接受同一地址和 Vault，防止复用旧同步游标。
#[test]
fn binding_rejects_switches() {
    let mut db = Database::open_in_memory().unwrap();
    db.bind_remote("https://example.test/dav/", "chatvault-a")
        .unwrap();
    db.bind_remote("https://example.test/dav", "chatvault-a")
        .unwrap();
    assert!(db
        .bind_remote("https://example.test/dav", "chatvault-b")
        .is_err());
    assert!(db
        .bind_remote("https://other.test/dav", "chatvault-a")
        .is_err());
    assert!(db
        .check_remote_binding("https://example.test/dav", "../a")
        .is_err());
    assert!(db
        .check_remote_binding("https://example.test/dav", "plain-a")
        .is_err());
}

/// 清空旧 Vault 绑定后可改绑新 Vault；本地索引保留。
#[test]
fn reset_vault_binding_allows_rebind() {
    use chatvault_core::models::{JournalEvent, JournalEventType};
    use chrono::Utc;

    let mut db = Database::open_in_memory().unwrap();
    db.bind_remote("https://example.test/dav", "chatvault-a")
        .unwrap();
    db.upsert_sync_cursor("local-dev", 1, 5).unwrap();
    db.set_setting("pending_segment_local-dev_1", "[]").unwrap();
    db.set_setting("epoch_local-dev", "2").unwrap();
    db.upsert_known_device(&chatvault_core::models::DeviceInfo {
        device_id: "local-dev".into(),
        display_name: Some("本机".into()),
        epoch: 2,
        last_seq: 5,
        updated_at: Utc::now(),
    })
    .unwrap();

    let time = Utc::now();
    let ev = JournalEvent {
        event_id: "evt-reset".into(),
        device_id: "local-dev".into(),
        epoch: 1,
        seq: 1,
        logical_clock: 1,
        schema_version: 1,
        event_type: JournalEventType::FileRecordAdded,
        payload: serde_json::json!({
            "object_id":"blake3:aa","hash":"aa","record_id":"r-reset","size":1,
            "source_type":"generic-folder","source_account_id":null,"source_conversation_id":null,
            "original_name":"a.txt","file_time":time.to_rfc3339(),"discovered_at":time.to_rfc3339(),"extension":"txt"
        }),
        created_at: time,
    };
    db.apply_file_record_added_event(&ev).unwrap();
    db.insert_journal_event_if_absent(&ev).unwrap();
    db.connection()
        .execute(
            "INSERT INTO applied_events(event_id, applied_at) VALUES ('remote-1', ?1)",
            [time.to_rfc3339()],
        )
        .unwrap();
    db.connection()
        .execute(
            "INSERT INTO upload_tasks(task_id,record_id,object_id,status,updated_at) VALUES ('t1','r-reset','blake3:aa','backed_up',?1)",
            [time.to_rfc3339()],
        )
        .unwrap();

    let report = db.reset_vault_binding().unwrap();
    assert!(report.had_binding);
    assert_eq!(report.cleared_cursors, 1);
    assert_eq!(report.cleared_journal_events, 1);
    assert_eq!(report.cleared_applied_events, 1);
    assert_eq!(report.cleared_devices, 1);
    assert_eq!(report.requeued_uploads, 1);

    assert!(db.get_setting("remote_binding").unwrap().is_none());
    assert!(db
        .get_setting("pending_segment_local-dev_1")
        .unwrap()
        .is_none());
    assert!(db.get_setting("epoch_local-dev").unwrap().is_none());
    assert_eq!(db.cursor_seq("local-dev", 1).unwrap(), 0);
    assert!(!db.event_already_applied("remote-1").unwrap());
    // 本地来源记录保留
    assert_eq!(db.get_stats().unwrap().total_records, 1);
    // 可改绑新 Vault
    db.bind_remote("https://example.test/dav", "chatvault-b")
        .unwrap();
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
            "source_type":"test","source_account_id":null,"source_conversation_id":null,
            "file_time":if seq<501 {"2026-09-11T00:00:00Z"}else{"2020-01-01T00:00:00Z"}}),
    }
}
