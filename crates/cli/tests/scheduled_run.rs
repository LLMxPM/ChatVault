// 定时 CLI 进程级回归：隔离数据库验证摘要、错误退出、日志追加与保留规则。
use chatvault_index::Database;
use chrono::{Days, Utc};
use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};

/// 以与计划任务相同的 CLI 参数执行；测试数据全部位于临时目录。
fn execute(db: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chatvault-cli"))
        .arg("scheduled-run")
        .arg("--db")
        .arg(db)
        .env("RUST_LOG", "off")
        .output()
        .unwrap()
}

/// 读取所有按日追加的日志，避免测试恰好跨 UTC 午夜时出现假失败。
fn logs(root: &Path) -> String {
    std::fs::read_dir(root.join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("scheduled-")
        })
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 零新增也是有效统计；重复运行应追加日志，清理只影响过期的定时日志。
#[test]
fn successful_run_records_summary_and_appends_logs() {
    let root = tempfile::Builder::new()
        .prefix("chatvault 定时 ' 测试 ")
        .tempdir()
        .unwrap();
    let db_path = root.path().join("资料库.db");
    let log_dir = root.path().join("logs");
    std::fs::create_dir(&log_dir).unwrap();
    let expired = Utc::now()
        .date_naive()
        .checked_sub_days(Days::new(8))
        .unwrap();
    let stale = log_dir.join(format!("scheduled-{expired}.log"));
    std::fs::write(&stale, "expired").unwrap();
    std::fs::write(log_dir.join("desktop.log"), "keep").unwrap();
    let output = execute(&db_path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let db = Database::open(&db_path).unwrap();
    let run = db.list_task_runs(1).unwrap().remove(0);
    assert_eq!(run.status, "success");
    assert_eq!(run.trigger_source, "schedule");
    let summary: Value = serde_json::from_str(run.summary_json.as_ref().unwrap()).unwrap();
    assert_eq!(summary["scan"]["newObjects"], 0);
    assert!(summary["archive"].is_null());
    assert!(summary.get("newObjects").is_none());
    assert!(db.get_active_task_run().unwrap().is_none());
    assert!(execute(&db_path).status.success());
    assert_eq!(db.list_task_runs(10).unwrap().len(), 2);
    let content = logs(root.path());
    assert!(content.contains(&run.run_id));
    assert!(content.contains("资料库.db"));
    assert_eq!(content.matches("定时运行结束").count(), 2);
    assert!(!content.contains('\u{1b}'));
    assert!(!stale.exists());
    assert_eq!(
        std::fs::read_to_string(log_dir.join("desktop.log")).unwrap(),
        "keep"
    );
}

/// 配置解析失败必须同时返回非零退出码、写日志并结束数据库中的运行和阶段。
#[test]
fn malformed_settings_fail_with_persisted_reason() {
    for key in ["collect_sources", "collect_selected_accounts"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("chatvault.db");
        let mut db = Database::open(&path).unwrap();
        db.set_setting(key, "invalid json").unwrap();
        let output = execute(&path);
        assert!(!output.status.success());
        let run = db.list_task_runs(1).unwrap().remove(0);
        assert_eq!(run.status, "failed");
        assert!(run.error_message.as_ref().unwrap().contains("解析"));
        assert_eq!(
            db.list_task_run_stages(&run.run_id).unwrap()[0].status,
            "failed"
        );
        assert!(db.get_active_task_run().unwrap().is_none());
        let content = logs(root.path());
        assert!(content.contains("CLI 执行失败"));
        assert!(content.contains(run.error_message.as_ref().unwrap()));
    }
}

/// 数据库尚未打开就失败时，文件日志仍须留下错误，不能只依赖运行历史。
#[test]
fn database_open_failure_is_logged() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("chatvault.db");
    std::fs::write(&path, "not a sqlite database").unwrap();
    assert!(!execute(&path).status.success());
    assert!(logs(root.path()).contains("CLI 执行失败"));
}
