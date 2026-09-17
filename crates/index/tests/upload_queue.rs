// 上传队列删除回归测试：限定缺失状态、保留文件与历史、拒绝过期操作并保证事务回滚。
use chatvault_core::models::{DiscoveredFile, TaskRunKind};
use chatvault_index::{Database, NewTaskRunItem};
use chrono::Utc;

/// 使用真实临时文件入库，返回隔离目录、数据库及其队列任务 ID。
fn fixture() -> (tempfile::TempDir, Database, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"original").unwrap();
    let mut db = Database::open(dir.path().join("index.db")).unwrap();
    db.ingest_file(
        &DiscoveredFile {
            source_type: "test".into(),
            source_account_id: None,
            source_conversation_id: None,
            absolute_path: path.to_string_lossy().into(),
            file_name: "file.txt".into(),
            file_size: 8,
            modified_time: Utc::now(),
        },
        "device",
    )
    .unwrap();
    let task_id = db.pending_uploads().unwrap()[0].task_id.clone();
    (dir, db, task_id)
}

/// 记录缺失历史，用于验证删除仅解除重试关联。
fn add_history(db: &mut Database, task_id: &str) -> String {
    let run_id = db
        .start_task_run(TaskRunKind::Pipeline, "manual", true, "desktop")
        .unwrap();
    db.add_task_run_item(
        &run_id,
        &NewTaskRunItem {
            stage: "archive",
            record_id: None,
            object_id: None,
            task_id: Some(task_id),
            name: "file.txt",
            status: "missing",
            error_code: Some("MISSING"),
            error_message: Some("上传来源缺失"),
            size: Some(8),
        },
    )
    .unwrap();
    run_id
}

/// 删除队列项仍保留索引、原文件、受控副本、同步日志和历史错误信息。
#[test]
fn delete_missing_task_preserves_files_and_history() {
    let (dir, mut db, task_id) = fixture();
    let cached = db.upload_source(&task_id).unwrap();
    let run_id = add_history(&mut db, &task_id);
    db.update_task_status(&task_id, "missing", Some("上传来源缺失"))
        .unwrap();
    let events_before: i64 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM journal_events", [], |r| r.get(0))
        .unwrap();

    assert!(db.delete_missing_upload_task(&task_id).unwrap());
    assert!(db.list_upload_tasks(None, 100).unwrap().is_empty());
    assert!(db.pending_uploads().unwrap().is_empty());
    let stats = db.get_stats().unwrap();
    assert_eq!(
        (stats.total_objects, stats.total_records, stats.local_files),
        (1, 1, 1)
    );
    assert_eq!(
        std::fs::read(dir.path().join("file.txt")).unwrap(),
        b"original"
    );
    assert_eq!(std::fs::read(cached).unwrap(), b"original");
    let events_after: i64 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM journal_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(events_before, events_after);
    let items = db.list_task_run_items(&run_id, None, None, 100).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].task_id.is_none());
    assert_eq!(items[0].status, "missing");
    assert_eq!(items[0].error_message.as_deref(), Some("上传来源缺失"));
    assert!(!db.delete_missing_upload_task(&task_id).unwrap());
    assert!(db.requeue_task(&task_id).is_err());
    drop(db);
    let reopened = Database::open(dir.path().join("index.db")).unwrap();
    assert!(reopened.list_upload_tasks(None, 100).unwrap().is_empty());
}

/// 限定任务 ID 和最新状态，防止旧页面删除已重试、已暂停或已完成的任务。
#[test]
fn delete_missing_task_rejects_other_states_and_ids() {
    let (_dir, mut db, task_id) = fixture();
    let run_id = add_history(&mut db, &task_id);
    for status in [
        "queued",
        "retryable_failed",
        "paused",
        "backed_up",
        "cancelled",
    ] {
        db.update_task_status(&task_id, status, None).unwrap();
        assert!(!db.delete_missing_upload_task(&task_id).unwrap());
        assert_eq!(db.list_upload_tasks(None, 100).unwrap()[0].status, status);
        assert_eq!(
            db.list_task_run_items(&run_id, None, None, 100).unwrap()[0]
                .task_id
                .as_deref(),
            Some(task_id.as_str())
        );
    }
    db.update_task_status(&task_id, "missing", None).unwrap();
    assert!(!db.delete_missing_upload_task("' OR 1=1 --").unwrap());
    db.requeue_task(&task_id).unwrap();
    assert!(!db.delete_missing_upload_task(&task_id).unwrap());
    assert!(db.task_is_runnable(&task_id).unwrap());
}

/// 解除历史关联失败时必须回滚删除，避免队列与运行历史出现半完成状态。
#[test]
fn delete_missing_task_rolls_back_on_history_failure() {
    let (_dir, mut db, task_id) = fixture();
    let run_id = add_history(&mut db, &task_id);
    db.update_task_status(&task_id, "missing", None).unwrap();
    db.connection()
        .execute_batch(
            "CREATE TRIGGER reject_history_update BEFORE UPDATE ON task_run_items
         BEGIN SELECT RAISE(ABORT, '模拟历史写入失败'); END;",
        )
        .unwrap();
    assert!(db.delete_missing_upload_task(&task_id).is_err());
    assert_eq!(
        db.list_upload_tasks(None, 100).unwrap()[0].status,
        "missing"
    );
    assert_eq!(
        db.list_task_run_items(&run_id, None, None, 100).unwrap()[0]
            .task_id
            .as_deref(),
        Some(task_id.as_str())
    );
}
