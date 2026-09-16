// ChatVault 同步故障回归：验证双设备、事务回滚、不可变提交和远端完整性。
mod support;
use chatvault_index::Database;
use chatvault_sync::{publish_pending_events, pull_and_apply, JournalPublisher};
use support::*;

/// 数据库应用中途失败时整片回滚；去除故障后重试完整恢复，重复拉取幂等。
#[tokio::test]
async fn failed_segment_rolls_back_and_retries() {
    let server = Server::new("d").await;
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    for seq in 1..=3 {
        add_source(&mut source, &event(seq), true);
    }
    publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
        .await
        .unwrap();
    let mut target = Database::open_in_memory().unwrap();
    target.connection().execute_batch("CREATE TRIGGER reject_record BEFORE INSERT ON file_records WHEN NEW.record_id='r2' BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .is_err()
    );
    assert_eq!(target.get_stats().unwrap().total_records, 0);
    assert_eq!(target.cursor_seq("a", 1).unwrap(), 0);
    assert!(!target.event_already_applied("e1").unwrap());
    target
        .connection()
        .execute_batch("DROP TRIGGER reject_record")
        .unwrap();
    assert_eq!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .unwrap(),
        0
    );
    assert_eq!(target.cursor_seq("a", 1).unwrap(), 3);
}

/// 未上传对象的元数据不能进入 commit；错误的已完成状态还需经远端校验。
#[tokio::test]
async fn unready_or_missing_objects_cannot_publish() {
    let server = Server::new("D").await;
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), false);
    assert_eq!(
        publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
            .await
            .unwrap(),
        0
    );
    assert!(!server
        .state
        .lock()
        .unwrap()
        .files
        .keys()
        .any(|p| p.contains("/commits/")));
    source.update_task_status("te1", "backed_up", None).unwrap();
    assert!(
        publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
            .await
            .is_err()
    );
    assert_eq!(source.cursor_seq("a", 1).unwrap(), 0);
}

/// 发布阶段对已 backed_up 对象只做 HEAD 轻量门禁，不再整份 GET 回读。
#[tokio::test]
async fn publish_preflight_uses_head_not_object_get() {
    let server = Server::new("P").await;
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
        .await
        .unwrap();
    let object_path = format!(
        "/{}",
        chatvault_metadata::get_object_path("chatvault-v", &hash())
    );
    let state = server.state.lock().unwrap();
    let gets = state
        .calls
        .get(&format!("GET {object_path}"))
        .copied()
        .unwrap_or(0);
    let heads = state
        .calls
        .get(&format!("HEAD {object_path}"))
        .copied()
        .unwrap_or(0);
    assert!(heads >= 1, "发布应至少 HEAD 一次对象路径");
    assert_eq!(gets, 0, "发布不应整份 GET 回读对象");
}

/// 发布时远端对象大小与事件声明不一致会失败且不推进游标。
#[tokio::test]
async fn publish_preflight_rejects_size_mismatch() {
    let server = Server::new("P2").await;
    server.seed_object();
    let path = format!(
        "/{}",
        chatvault_metadata::get_object_path("chatvault-v", &hash())
    );
    // 同路径塞入错误长度内容，模拟远端被改写
    server
        .state
        .lock()
        .unwrap()
        .files
        .insert(path, b"nope".to_vec());
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    assert!(
        publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
            .await
            .is_err()
    );
    assert_eq!(source.cursor_seq("a", 1).unwrap(), 0);
}

/// HEAD 不带 Content-Length 时降级完整回读；内容正确则发布成功并确实 GET 了对象。
#[tokio::test]
async fn publish_falls_back_to_get_when_head_omits_content_length() {
    let server = Server::new("P3").await;
    server.omit_head_content_length();
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
        .await
        .unwrap();
    assert_eq!(source.cursor_seq("a", 1).unwrap(), 1);
    let object_path = format!(
        "/{}",
        chatvault_metadata::get_object_path("chatvault-v", &hash())
    );
    let state = server.state.lock().unwrap();
    let gets = state
        .calls
        .get(&format!("GET {object_path}"))
        .copied()
        .unwrap_or(0);
    assert!(gets >= 1, "无 Content-Length 时应降级 GET 回读对象");
}

/// HEAD 不带 Content-Length 且等长内容被篡改时，降级回读应因哈希不匹配失败。
#[tokio::test]
async fn publish_fallback_rejects_hash_mismatch_without_content_length() {
    let server = Server::new("P4").await;
    server.omit_head_content_length();
    server.seed_object();
    let path = format!(
        "/{}",
        chatvault_metadata::get_object_path("chatvault-v", &hash())
    );
    // 与 b"content" 等长，仅内容错误：长度门禁挡不住，必须靠降级 BLAKE3
    server
        .state
        .lock()
        .unwrap()
        .files
        .insert(path, b"XXXXXXX".to_vec());
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    assert!(
        publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
            .await
            .is_err()
    );
    assert_eq!(source.cursor_seq("a", 1).unwrap(), 0);
}

/// 远端对象被损坏后恢复失败且不推进游标；修复对象后能继续。
#[tokio::test]
async fn pull_requires_readable_objects() {
    let server = Server::new("D").await;
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
        .await
        .unwrap();
    let path = format!(
        "/{}",
        chatvault_metadata::get_object_path("chatvault-v", &hash())
    );
    server
        .state
        .lock()
        .unwrap()
        .files
        .insert(path, b"wrong".to_vec());
    let mut target = Database::open_in_memory().unwrap();
    assert!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .is_err()
    );
    assert_eq!(target.cursor_seq("a", 1).unwrap(), 0);
    server.seed_object();
    assert_eq!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .unwrap(),
        1
    );
}

/// 提交已上传但响应丢失时，重试复用原分片，不把随后扫描的事件混入原提交。
#[tokio::test]
async fn uncertain_commit_is_retried_without_rewriting() {
    let server = Server::new("D").await;
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    server.fail("PUT", "chatvault-v/commits/a/1/1.json", 1, true);
    assert!(
        publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
            .await
            .is_err()
    );
    let original = server.state.lock().unwrap().files["/chatvault-v/commits/a/1/1.json"].clone();
    add_source(&mut source, &event(2), true);
    assert_eq!(
        publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        server.state.lock().unwrap().files["/chatvault-v/commits/a/1/1.json"],
        original
    );
    let mut target = Database::open_in_memory().unwrap();
    assert_eq!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .unwrap(),
        2
    );
}

/// 游标写入后设备注册失败，空队列重试也会重新注册。
#[tokio::test]
async fn registration_failure_is_repaired_without_new_events() {
    let server = Server::new("D").await;
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    server.fail("PUT", "chatvault-v/devices/a.json", 2, false);
    assert!(
        publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
            .await
            .is_err()
    );
    assert_eq!(source.cursor_seq("a", 1).unwrap(), 1);
    assert_eq!(
        publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
            .await
            .unwrap(),
        1
    );
    let value: serde_json::Value =
        serde_json::from_slice(&server.state.lock().unwrap().files["/chatvault-v/devices/a.json"])
            .unwrap();
    assert_eq!(value["last_seq"], 1);
}

/// 新 epoch 从 1 开始且不会遮蔽历史 epoch，新设备能恢复全部记录。
#[tokio::test]
async fn restore_includes_all_epochs() {
    let server = Server::new("D").await;
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
        .await
        .unwrap();
    let mut next = event(2);
    next.epoch = 2;
    next.seq = 1;
    add_source(&mut source, &next, true);
    JournalPublisher::save_epoch(&mut source, "a", 2).unwrap();
    publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
        .await
        .unwrap();
    let mut target = Database::open_in_memory().unwrap();
    assert_eq!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .unwrap(),
        2
    );
    assert_eq!(target.cursor_seq("a", 1).unwrap(), 1);
    assert_eq!(target.cursor_seq("a", 2).unwrap(), 1);
}

/// 远端鉴权或服务错误必须报告失败，不能伪装为无设备。
#[tokio::test]
async fn list_failure_is_reported() {
    let server = Server::new("D").await;
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
        .await
        .unwrap();
    server.fail("PROPFIND", "chatvault-v/devices", 1, false);
    let mut target = Database::open_in_memory().unwrap();
    assert!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .is_err()
    );
}

/// 已存在但错误的远端对象不能使任务进入 backed_up，手动重试也不能绕过校验。
#[tokio::test]
async fn existing_corruption_never_counts_as_verified() {
    let server = Server::new("D").await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"content").unwrap();
    let mut db = Database::open_in_memory().unwrap();
    let file = chatvault_core::models::DiscoveredFile {
        source_type: "test".into(),
        source_account_id: None,
        absolute_path: path.to_string_lossy().into(),
        file_name: "file.txt".into(),
        file_size: 7,
        modified_time: chrono::Utc::now(),
        source_conversation_id: None,
    };
    db.ingest_file(&file, "a").unwrap();
    let task = db.pending_uploads().unwrap()[0].task_id.clone();
    let remote = format!(
        "/{}",
        chatvault_metadata::get_object_path("chatvault-v", &hash())
    );
    server
        .state
        .lock()
        .unwrap()
        .files
        .insert(remote, b"bad".to_vec());
    for _ in 0..2 {
        let report = chatvault_sync::archive::archive_pending(
            &server.client,
            &mut db,
            "chatvault-v",
            "a",
            0,
        )
        .await
        .unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(report.verified, 0);
        assert_eq!(
            db.list_upload_tasks(None, 10).unwrap()[0].status,
            "retryable_failed"
        );
        db.requeue_task(&task).unwrap();
    }
    server.seed_object();
    std::fs::remove_file(path).unwrap();
    let report =
        chatvault_sync::archive::archive_pending(&server.client, &mut db, "chatvault-v", "a", 0)
            .await
            .unwrap();
    assert_eq!(report.verified, 1);
    assert_eq!(report.failed, 0);
    publish_pending_events(&server.client, &mut db, "chatvault-v", "a")
        .await
        .unwrap();
    let mut target = Database::open_in_memory().unwrap();
    assert_eq!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .unwrap(),
        1
    );
}

/// 原始来源删除后，仍能从受控副本完成 PUT、回读和 MOVE。
#[tokio::test]
async fn archive_uses_snapshot_after_source_deletion() {
    let server = Server::new("D").await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"content").unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting("cache_retention_days", "0").unwrap();
    db.ingest_file(
        &chatvault_core::models::DiscoveredFile {
            source_type: "test".into(),
            source_account_id: None,
            absolute_path: path.to_string_lossy().into(),
            file_name: "file.txt".into(),
            file_size: 7,
            modified_time: chrono::Utc::now(),
            source_conversation_id: None,
        },
        "a",
    )
    .unwrap();
    let task = db.pending_uploads().unwrap().remove(0);
    let cache = db.upload_source(&task.task_id).unwrap();
    std::fs::remove_file(path).unwrap();
    let report =
        chatvault_sync::archive::archive_pending(&server.client, &mut db, "chatvault-v", "a", 0)
            .await
            .unwrap();
    assert_eq!(report.uploaded, 1);
    assert_eq!(report.verified, 1);
    assert_eq!(
        db.list_upload_tasks(None, 10).unwrap()[0].status,
        "backed_up"
    );
    assert!(cache.exists());
    publish_pending_events(&server.client, &mut db, "chatvault-v", "a")
        .await
        .unwrap();
    assert!(!cache.exists());
    let mut target = Database::open_in_memory().unwrap();
    assert_eq!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .unwrap(),
        1
    );
}

/// 不兼容 Vault 在写入对象和绑定本机索引之前被拒绝。
#[tokio::test]
async fn incompatible_vault_does_not_bind_local_index() {
    let server = Server::new("D").await;
    server.state.lock().unwrap().files.insert("/chatvault-v/config/vault.json".into(),serde_json::to_vec(&serde_json::json!({
        "vault_id":"chatvault-v","format_version":99,"hash_algorithm":"blake3","created_at":"2026-09-11T00:00:00Z"})).unwrap());
    let mut db = Database::open_in_memory().unwrap();
    assert!(
        publish_pending_events(&server.client, &mut db, "chatvault-v", "a")
            .await
            .is_err()
    );
    assert!(db.get_setting("remote_binding").unwrap().is_none());
    assert_eq!(server.state.lock().unwrap().files.len(), 1);
}

/// 篡改 commit 中的事件数量后必须停止恢复，不接受只通过哈希的声明。
#[tokio::test]
async fn inconsistent_commit_does_not_advance_cursor() {
    let server = Server::new("D").await;
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    add_source(&mut source, &event(1), true);
    publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
        .await
        .unwrap();
    {
        let mut store = server.state.lock().unwrap();
        let marker = store
            .files
            .get_mut("/chatvault-v/commits/a/1/1.json")
            .unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(marker).unwrap();
        value["event_count"] = serde_json::json!(2);
        *marker = serde_json::to_vec(&value).unwrap();
    }
    let mut target = Database::open_in_memory().unwrap();
    assert!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .is_err()
    );
    assert_eq!(target.get_stats().unwrap().total_records, 0);
    assert_eq!(target.cursor_seq("a", 1).unwrap(), 0);
}

/// 超过单片容量的本机事件会分批全部发布，恢复不会停在第 500 条。
#[tokio::test]
async fn publish_drains_multiple_segments() {
    let server = Server::new("D").await;
    server.seed_object();
    let mut source = Database::open_in_memory().unwrap();
    for seq in 1..=501 {
        add_source(&mut source, &event(seq), true);
    }
    assert_eq!(
        publish_pending_events(&server.client, &mut source, "chatvault-v", "a")
            .await
            .unwrap(),
        501
    );
    let mut target = Database::open_in_memory().unwrap();
    assert_eq!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .unwrap(),
        501
    );
}

/// 无副本的文件可完成直接上传和元数据发布，且保留原附件。
#[tokio::test]
async fn archive_direct_source_and_publish() {
    let server = Server::new("D").await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"content").unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.set_setting("copy_threshold_mib", "0").unwrap();
    db.ingest_file(
        &chatvault_core::models::DiscoveredFile {
            source_type: "test".into(),
            source_account_id: None,
            absolute_path: path.to_string_lossy().into(),
            file_name: "file.txt".into(),
            file_size: 7,
            modified_time: chrono::Utc::now(),
            source_conversation_id: None,
        },
        "a",
    )
    .unwrap();
    let task = db.pending_uploads().unwrap().remove(0);
    assert_eq!(
        std::fs::read(db.upload_source(&task.task_id).unwrap()).unwrap(),
        b"content"
    );
    let report =
        chatvault_sync::archive::archive_pending(&server.client, &mut db, "chatvault-v", "a", 0)
            .await
            .unwrap();
    assert_eq!(report.uploaded, 1);
    assert_eq!(report.verified, 1);
    assert_eq!(
        db.list_upload_tasks(None, 10).unwrap()[0].status,
        "backed_up"
    );
    assert!(path.exists());
    publish_pending_events(&server.client, &mut db, "chatvault-v", "a")
        .await
        .unwrap();
    assert!(path.exists());
    let mut target = Database::open_in_memory().unwrap();
    assert_eq!(
        pull_and_apply(&server.client, &mut target, "chatvault-v", "b")
            .await
            .unwrap(),
        1
    );
}
