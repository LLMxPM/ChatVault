// 定时摘要回归测试：验证异常与取消时保留阶段统计，并区分文件对象和来源记录。
use super::*;
use chatvault_core::models::TaskRunKind;

/// 构造扫描已完成的运行，特意设置来源记录数大于内容对象数以验证去重口径。
fn scanned_run() -> (Database, String) {
    let mut db = Database::open_in_memory().unwrap();
    let run = db
        .start_task_run(TaskRunKind::Pipeline, "schedule", true, "cli")
        .unwrap();
    db.start_task_run_stage(&run, TaskRunStageName::Scan)
        .unwrap();
    let stats = scan_stats(&ScanReport {
        discovered: 8,
        indexed: 5,
        new_objects: 2,
        skipped: 3,
    });
    db.finish_task_run_stage(
        &run,
        TaskRunStageName::Scan,
        TaskRunStageStatus::Success,
        Some(&stats.to_string()),
        None,
    )
    .unwrap();
    (db, run)
}

/// 同步中断必须结束运行，保留扫描与归档结果，且重复错误收尾不能覆盖原状态。
#[test]
fn sync_failure_preserves_nested_summary() {
    let (mut db, run) = scanned_run();
    db.start_task_run_stage(&run, TaskRunStageName::Archive)
        .unwrap();
    db.finish_task_run_stage(
        &run,
        TaskRunStageName::Archive,
        TaskRunStageStatus::Success,
        Some(r#"{"uploaded":2,"verified":2,"failed":0,"processed":2}"#),
        None,
    )
    .unwrap();
    db.start_task_run_stage(&run, TaskRunStageName::Pull)
        .unwrap();
    finish_error(&mut db, &run, &anyhow::anyhow!("远端不可达")).unwrap();
    finish_error(&mut db, &run, &anyhow::anyhow!("不应覆盖")).unwrap();
    let row = db.get_task_run(&run).unwrap().unwrap();
    assert_eq!(row.status, "partial");
    assert_eq!(row.error_message.as_deref(), Some("远端不可达"));
    let summary: Value = serde_json::from_str(row.summary_json.as_ref().unwrap()).unwrap();
    assert_eq!(summary["scan"]["newObjects"], 2);
    assert_eq!(summary["archive"]["uploaded"], 2);
    assert!(summary.get("newObjects").is_none());
    let stages = db.list_task_run_stages(&run).unwrap();
    assert_eq!(
        stages.iter().find(|s| s.stage == "pull").unwrap().status,
        "failed"
    );
    assert!(db.get_active_task_run().unwrap().is_none());
}

/// 取消归档后已有统计应保留在 archive 下，同时保持 cancelled 终态。
#[test]
fn cancellation_preserves_completed_work() {
    let (mut db, run) = scanned_run();
    db.start_task_run_stage(&run, TaskRunStageName::Archive)
        .unwrap();
    finish_cancelled(
        &mut db,
        &run,
        TaskRunStageName::Archive,
        &json!({ "uploaded": 1, "verified": 1, "failed": 0, "processed": 1 }),
    )
    .unwrap();
    let row = db.get_task_run(&run).unwrap().unwrap();
    assert_eq!(row.status, "cancelled");
    let summary: Value = serde_json::from_str(row.summary_json.as_ref().unwrap()).unwrap();
    assert_eq!(summary["scan"]["newObjects"], 2);
    assert_eq!(summary["archive"]["uploaded"], 1);
    assert_eq!(summary["cancelled"], true);
    assert!(db.get_active_task_run().unwrap().is_none());
}
