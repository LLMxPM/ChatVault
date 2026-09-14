// ChatVault 对象隐藏/恢复/彻底删除与查询过滤回归测试。
use chatvault_core::models::{DiscoveredFile, JournalEvent, JournalEventType};
use chatvault_index::{Database, IngestResult, SearchFilter, SearchService};
use chrono::Utc;

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

fn ingest_one(dir: &std::path::Path, name: &str, content: &[u8]) -> (Database, String) {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    let mut db = Database::open(dir.join("index.db")).unwrap();
    let result = db.ingest_file(&discovered(&path), "dev-a").unwrap();
    let object_id = match result {
        IngestResult::Indexed { object_id, .. } => object_id,
        _ => panic!("应成功入库"),
    };
    (db, object_id)
}

fn search_default(db: &Database) -> usize {
    SearchService::new(db)
        .search_objects(&SearchFilter::default(), "dev-a")
        .unwrap()
        .total
}

fn search_hidden(db: &Database) -> usize {
    SearchService::new(db)
        .search_objects(
            &SearchFilter {
                hidden_only: true,
                ..Default::default()
            },
            "dev-a",
        )
        .unwrap()
        .total
}

#[test]
fn hide_restore_updates_default_and_hidden_lists() {
    let dir = tempfile::tempdir().unwrap();
    let (mut db, object_id) = ingest_one(dir.path(), "a.txt", b"hello");
    assert_eq!(search_default(&db), 1);
    assert_eq!(search_hidden(&db), 0);

    db.hide_object("dev-a", &object_id).unwrap();
    assert_eq!(search_default(&db), 0);
    assert_eq!(search_hidden(&db), 1);
    // 幂等
    db.hide_object("dev-a", &object_id).unwrap();

    db.restore_object("dev-a", &object_id).unwrap();
    assert_eq!(search_default(&db), 1);
    assert_eq!(search_hidden(&db), 0);
    // 关键词可搜回
    let page = SearchService::new(&db)
        .search_objects(
            &SearchFilter {
                keyword: Some("a.txt".into()),
                ..Default::default()
            },
            "dev-a",
        )
        .unwrap();
    assert_eq!(page.total, 1);
}

#[test]
fn purge_requires_hidden_and_removes_records() {
    let dir = tempfile::tempdir().unwrap();
    let (mut db, object_id) = ingest_one(dir.path(), "b.txt", b"bye");
    assert!(db.purge_object("dev-a", &object_id).is_err());

    db.hide_object("dev-a", &object_id).unwrap();
    let outcome = db.purge_object("dev-a", &object_id).unwrap();
    assert_eq!(outcome.object_id, object_id);
    assert!(db.object_is_purged(&object_id).unwrap());
    assert_eq!(search_default(&db), 0);
    assert_eq!(search_hidden(&db), 0);
    assert!(db.purge_object("dev-a", &object_id).is_err());
    assert!(db.restore_object("dev-a", &object_id).is_err());
}

/// purge 后默认进入待远端清理列表；标记成功后不再重复。
#[test]
fn purge_tracks_pending_remote_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let (mut db, object_id) = ingest_one(dir.path(), "r.txt", b"residual");
    db.hide_object("dev-a", &object_id).unwrap();
    db.purge_object("dev-a", &object_id).unwrap();
    assert_eq!(db.count_pending_remote_purges().unwrap(), 1);
    let pending = db.list_pending_remote_purges(10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].object_id, object_id);
    assert!(!pending[0].hash.is_empty());

    db.mark_remote_purged_cleaned(&object_id).unwrap();
    assert_eq!(db.count_pending_remote_purges().unwrap(), 0);
    assert!(db.list_pending_remote_purges(10).unwrap().is_empty());
    // 幂等：重复标记不报错
    db.mark_remote_purged_cleaned(&object_id).unwrap();
}

#[test]
fn purge_allows_reingest_same_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.txt");
    std::fs::write(&path, b"same").unwrap();
    let mut db = Database::open(dir.path().join("index.db")).unwrap();
    let first = match db.ingest_file(&discovered(&path), "dev-a").unwrap() {
        IngestResult::Indexed { object_id, .. } => object_id,
        _ => panic!("应成功入库"),
    };
    db.hide_object("dev-a", &first).unwrap();
    db.purge_object("dev-a", &first).unwrap();

    // 清除 local_files 后同路径再次入库
    db.connection()
        .execute("DELETE FROM local_files", [])
        .unwrap();
    let second = match db.ingest_file(&discovered(&path), "dev-a").unwrap() {
        IngestResult::Indexed {
            object_id,
            record_id,
            ..
        } => {
            assert!(!record_id.is_empty());
            object_id
        }
        IngestResult::Skipped { path } => panic!("应重新入库，却跳过: {path}"),
    };
    assert_eq!(second, first);
    assert_eq!(search_default(&db), 1);
}

#[test]
fn remote_purge_event_clears_local_records() {
    let dir = tempfile::tempdir().unwrap();
    let (mut db, object_id) = ingest_one(dir.path(), "d.txt", b"remote");
    db.hide_object("dev-a", &object_id).unwrap();
    let ev = JournalEvent {
        event_id: "purge-1".into(),
        device_id: "dev-b".into(),
        epoch: 1,
        seq: 1,
        logical_clock: 100,
        schema_version: 1,
        event_type: JournalEventType::ObjectPurged,
        payload: serde_json::json!({ "object_id": object_id }),
        created_at: Utc::now(),
    };
    db.apply_object_purged_event(&ev).unwrap();
    assert!(db.object_is_purged(&object_id).unwrap());
    assert_eq!(search_default(&db), 0);
    assert_eq!(search_hidden(&db), 0);
}

#[test]
fn remote_hide_before_add_still_hides_after_add() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path().join("index.db")).unwrap();
    let object_id = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let hide = JournalEvent {
        event_id: "hide-1".into(),
        device_id: "dev-b".into(),
        epoch: 1,
        seq: 1,
        logical_clock: 2,
        schema_version: 1,
        event_type: JournalEventType::ObjectHidden,
        payload: serde_json::json!({ "object_id": object_id }),
        created_at: Utc::now(),
    };
    db.apply_object_hidden_event(&hide).unwrap();

    let added = JournalEvent {
        event_id: "add-1".into(),
        device_id: "dev-b".into(),
        epoch: 1,
        seq: 2,
        logical_clock: 1,
        schema_version: 1,
        event_type: JournalEventType::FileRecordAdded,
        payload: serde_json::json!({
            "object_id": object_id,
            "record_id": "r1",
            "hash": "a".repeat(64),
            "size": 3,
            "mime": "application/octet-stream",
            "extension": "bin",
            "object_created_at": Utc::now().to_rfc3339(),
            "source_type": "test",
            "source_account_id": null,
            "source_conversation_id": null,
            "original_name": "late.bin",
            "file_time": Utc::now().to_rfc3339(),
            "time_source": "mtime",
            "discovered_at": Utc::now().to_rfc3339(),
            "device_id": "dev-b",
        }),
        created_at: Utc::now(),
    };
    db.apply_file_record_added_event(&added).unwrap();
    assert_eq!(search_default(&db), 0);
    assert_eq!(search_hidden(&db), 1);
}
