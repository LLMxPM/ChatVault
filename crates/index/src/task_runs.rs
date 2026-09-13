// ChatVault 任务运行日志：持久化一次流水线/恢复的汇总、阶段与关键文件明细。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::{
    TaskRun, TaskRunItem, TaskRunItemStatus, TaskRunKind, TaskRunStage, TaskRunStageName,
    TaskRunStageStatus, TaskRunStatus,
};
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

/// 运行列表行
#[derive(Debug, Clone)]
pub struct TaskRunRow {
    pub run_id: String,
    pub kind: String,
    pub trigger_source: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub webdav_configured: bool,
    pub summary_json: Option<String>,
    pub error_message: Option<String>,
    pub runner_kind: String,
    pub runner_pid: Option<i64>,
    pub cancel_requested: bool,
    pub heartbeat_at: Option<String>,
}

/// 任务运行阶段行
#[derive(Debug, Clone)]
pub struct TaskRunStageRow {
    pub stage_id: String,
    pub run_id: String,
    pub stage: String,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub stats_json: Option<String>,
    pub message: Option<String>,
}

/// 任务运行明细行
#[derive(Debug, Clone)]
pub struct TaskRunItemRow {
    pub item_id: String,
    pub run_id: String,
    pub stage: String,
    pub record_id: Option<String>,
    pub object_id: Option<String>,
    pub task_id: Option<String>,
    pub name: String,
    pub status: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub size: Option<i64>,
    pub updated_at: String,
}

/// 新建明细项入参
#[derive(Debug, Clone)]
pub struct NewTaskRunItem<'a> {
    pub stage: &'a str,
    pub record_id: Option<&'a str>,
    pub object_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub name: &'a str,
    pub status: &'a str,
    pub error_code: Option<&'a str>,
    pub error_message: Option<&'a str>,
    pub size: Option<i64>,
}

fn db_error(e: rusqlite::Error) -> ChatVaultError {
    ChatVaultError::Database(e.to_string())
}

fn parse_time(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| ChatVaultError::Database(format!("时间格式无效 {s}: {e}")))
}

fn optional_time(s: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    s.map(parse_time).transpose()
}

const TASK_RUN_COLUMNS: &str =
    "run_id,kind,trigger_source,status,started_at,finished_at,duration_ms, \
 webdav_configured,summary_json,error_message,runner_kind,runner_pid,cancel_requested,heartbeat_at";

fn map_task_run_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRunRow> {
    Ok(TaskRunRow {
        run_id: r.get(0)?,
        kind: r.get(1)?,
        trigger_source: r.get(2)?,
        status: r.get(3)?,
        started_at: r.get(4)?,
        finished_at: r.get(5)?,
        duration_ms: r.get(6)?,
        webdav_configured: r.get::<_, i64>(7)? != 0,
        summary_json: r.get(8)?,
        error_message: r.get(9)?,
        runner_kind: r.get(10)?,
        runner_pid: r.get(11)?,
        cancel_requested: r.get::<_, i64>(12)? != 0,
        heartbeat_at: r.get(13)?,
    })
}

impl Database {
    /// 创建一次运行（status=running），记录执行方与 PID
    pub fn start_task_run(
        &mut self,
        kind: TaskRunKind,
        trigger_source: &str,
        webdav_configured: bool,
        runner_kind: &str,
    ) -> Result<String> {
        let run_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let pid = std::process::id() as i64;
        self.conn
            .execute(
                "INSERT INTO task_runs (run_id,kind,trigger_source,status,started_at,webdav_configured,runner_kind,runner_pid,heartbeat_at)
                 VALUES (?1,?2,?3,'running',?4,?5,?6,?7,?4)",
                params![
                    run_id,
                    kind.as_str(),
                    trigger_source,
                    now,
                    webdav_configured as i64,
                    runner_kind,
                    pid
                ],
            )
            .map_err(db_error)?;
        Ok(run_id)
    }

    /// 节流更新运行心跳与当前阶段进度统计
    pub fn touch_task_run_progress(
        &mut self,
        run_id: &str,
        stage: TaskRunStageName,
        done: usize,
        total: usize,
        current_name: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let stats = serde_json::json!({
            "done": done,
            "total": total,
            "currentName": current_name,
        })
        .to_string();
        self.conn
            .execute(
                "UPDATE task_runs SET heartbeat_at=?1 WHERE run_id=?2 AND status='running'",
                params![now, run_id],
            )
            .map_err(db_error)?;
        self.conn
            .execute(
                "UPDATE task_run_stages SET stats_json=?1 WHERE run_id=?2 AND stage=?3 AND status='running'",
                params![stats, run_id, stage.as_str()],
            )
            .map_err(db_error)?;
        Ok(())
    }

    /// 请求取消运行（仅 running）
    pub fn request_cancel_task_run(&mut self, run_id: &str) -> Result<bool> {
        let n = self
            .conn
            .execute(
                "UPDATE task_runs SET cancel_requested=1 WHERE run_id=?1 AND status='running'",
                params![run_id],
            )
            .map_err(db_error)?;
        Ok(n > 0)
    }

    /// 读取取消标志
    pub fn task_run_cancel_requested(&self, run_id: &str) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT cancel_requested FROM task_runs WHERE run_id=?1",
                params![run_id],
                |r| Ok(r.get::<_, i64>(0)? != 0),
            )
            .map_err(db_error)
    }

    /// 当前是否有正在执行的运行（供桌面监视器展示）
    pub fn get_active_task_run(&self) -> Result<Option<TaskRunRow>> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {TASK_RUN_COLUMNS} FROM task_runs WHERE status='running' ORDER BY started_at DESC LIMIT 1"
                ),
                [],
                map_task_run_row,
            )
            .optional()
            .map_err(db_error)
    }

    /// 开始阶段（幂等：同 run 同 stage 重复调用进入 running）
    pub fn start_task_run_stage(
        &mut self,
        run_id: &str,
        stage: TaskRunStageName,
    ) -> Result<String> {
        let now = Utc::now().to_rfc3339();
        let stage_id = Uuid::new_v4().to_string();
        self.conn
            .execute(
                "INSERT INTO task_run_stages (stage_id,run_id,stage,status,started_at)
                 VALUES (?1,?2,?3,'running',?4)
                 ON CONFLICT(run_id,stage) DO UPDATE SET status='running',started_at=?4,finished_at=NULL,duration_ms=NULL,message=NULL,stats_json=NULL",
                params![stage_id, run_id, stage.as_str(), now],
            )
            .map_err(db_error)?;
        let id: String = self
            .conn
            .query_row(
                "SELECT stage_id FROM task_run_stages WHERE run_id=?1 AND stage=?2",
                params![run_id, stage.as_str()],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        Ok(id)
    }

    /// 结束阶段
    #[allow(clippy::too_many_arguments)]
    pub fn finish_task_run_stage(
        &mut self,
        run_id: &str,
        stage: TaskRunStageName,
        status: TaskRunStageStatus,
        stats_json: Option<&str>,
        message: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        let now_s = now.to_rfc3339();
        self.conn
            .execute(
                "UPDATE task_run_stages SET status=?1, finished_at=?2,
                   duration_ms=CAST((julianday(?2)-julianday(started_at))*86400000 AS INTEGER),
                   stats_json=?3, message=?4
                 WHERE run_id=?5 AND stage=?6",
                params![
                    status.as_str(),
                    now_s,
                    stats_json,
                    message,
                    run_id,
                    stage.as_str()
                ],
            )
            .map_err(db_error)?;
        Ok(())
    }

    /// 写入关键明细项
    pub fn add_task_run_item(&mut self, run_id: &str, item: &NewTaskRunItem<'_>) -> Result<String> {
        let item_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO task_run_items
                 (item_id,run_id,stage,record_id,object_id,task_id,name,status,error_code,error_message,size,updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    item_id,
                    run_id,
                    item.stage,
                    item.record_id,
                    item.object_id,
                    item.task_id,
                    item.name,
                    item.status,
                    item.error_code,
                    item.error_message,
                    item.size,
                    now
                ],
            )
            .map_err(db_error)?;
        Ok(item_id)
    }

    /// 更新 WebDAV 配置标记（扫描后才知道）
    pub fn set_task_run_webdav_configured(&mut self, run_id: &str, configured: bool) -> Result<()> {
        self.conn
            .execute(
                "UPDATE task_runs SET webdav_configured=?1 WHERE run_id=?2",
                params![configured as i64, run_id],
            )
            .map_err(db_error)?;
        Ok(())
    }

    /// 结束运行
    pub fn finish_task_run(
        &mut self,
        run_id: &str,
        status: TaskRunStatus,
        summary_json: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        let now_s = now.to_rfc3339();
        self.conn
            .execute(
                "UPDATE task_runs SET status=?1, finished_at=?2,
                   duration_ms=CAST((julianday(?2)-julianday(started_at))*86400000 AS INTEGER),
                   summary_json=?3, error_message=?4
                 WHERE run_id=?5",
                params![status.as_str(), now_s, summary_json, error_message, run_id],
            )
            .map_err(db_error)?;
        Ok(())
    }

    /// 列出最近运行
    pub fn list_task_runs(&self, limit: usize) -> Result<Vec<TaskRunRow>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {TASK_RUN_COLUMNS} FROM task_runs ORDER BY started_at DESC LIMIT ?1"
            ))
            .map_err(db_error)?;
        let rows = stmt
            .query_map(params![limit.min(200) as i64], map_task_run_row)
            .map_err(db_error)?;
        rows.map(|r| r.map_err(db_error)).collect()
    }

    /// 读取单次运行
    pub fn get_task_run(&self, run_id: &str) -> Result<Option<TaskRunRow>> {
        self.conn
            .query_row(
                &format!("SELECT {TASK_RUN_COLUMNS} FROM task_runs WHERE run_id=?1"),
                params![run_id],
                map_task_run_row,
            )
            .optional()
            .map_err(db_error)
    }

    /// 读取运行的阶段列表
    pub fn list_task_run_stages(&self, run_id: &str) -> Result<Vec<TaskRunStageRow>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT stage_id,run_id,stage,status,started_at,finished_at,duration_ms,stats_json,message
                 FROM task_run_stages WHERE run_id=?1 ORDER BY started_at, stage_id",
            )
            .map_err(db_error)?;
        let rows = stmt
            .query_map(params![run_id], |r| {
                Ok(TaskRunStageRow {
                    stage_id: r.get(0)?,
                    run_id: r.get(1)?,
                    stage: r.get(2)?,
                    status: r.get(3)?,
                    started_at: r.get(4)?,
                    finished_at: r.get(5)?,
                    duration_ms: r.get(6)?,
                    stats_json: r.get(7)?,
                    message: r.get(8)?,
                })
            })
            .map_err(db_error)?;
        rows.map(|r| r.map_err(db_error)).collect()
    }

    /// 读取运行明细，可按阶段/状态过滤
    pub fn list_task_run_items(
        &self,
        run_id: &str,
        stage: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TaskRunItemRow>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT item_id,run_id,stage,record_id,object_id,task_id,name,status,error_code,error_message,size,updated_at
                 FROM task_run_items
                 WHERE run_id=?1 AND (?2 IS NULL OR stage=?2) AND (?3 IS NULL OR status=?3)
                 ORDER BY updated_at DESC LIMIT ?4",
            )
            .map_err(db_error)?;
        let rows = stmt
            .query_map(params![run_id, stage, status, limit.min(500) as i64], |r| {
                Ok(TaskRunItemRow {
                    item_id: r.get(0)?,
                    run_id: r.get(1)?,
                    stage: r.get(2)?,
                    record_id: r.get(3)?,
                    object_id: r.get(4)?,
                    task_id: r.get(5)?,
                    name: r.get(6)?,
                    status: r.get(7)?,
                    error_code: r.get(8)?,
                    error_message: r.get(9)?,
                    size: r.get(10)?,
                    updated_at: r.get(11)?,
                })
            })
            .map_err(db_error)?;
        rows.map(|r| r.map_err(db_error)).collect()
    }

    /// 统计运行内关键项数量（按状态）
    pub fn count_task_run_items_by_status(&self, run_id: &str) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT status, count(*) FROM task_run_items WHERE run_id=?1 GROUP BY status")
            .map_err(db_error)?;
        let rows = stmt
            .query_map(params![run_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(db_error)?;
        rows.map(|r| r.map_err(db_error)).collect()
    }

    /// 启动时：将心跳过期的 running 标记为 failed（崩溃残留）
    ///
    /// 有新鲜心跳（默认 60s 内）的 running 一律不动，避免桌面启动误杀正在跑的 CLI 定时运行。
    pub fn mark_stale_running_task_runs(&mut self) -> Result<usize> {
        let now = Utc::now();
        let now_s = now.to_rfc3339();
        let stale_heartbeat = (now - chrono::Duration::seconds(60)).to_rfc3339();
        let n = self
            .conn
            .execute(
                "UPDATE task_runs SET status='failed', finished_at=?1,
                   error_message=COALESCE(error_message,'应用启动时发现上次运行未正常结束（可能被强制关闭或崩溃）'),
                   duration_ms=CAST((julianday(?1)-julianday(started_at))*86400000 AS INTEGER)
                 WHERE status='running'
                   AND (heartbeat_at IS NULL OR heartbeat_at < ?2)",
                params![now_s, stale_heartbeat],
            )
            .map_err(db_error)?;
        self.conn
            .execute(
                "UPDATE task_run_stages SET status='failed', finished_at=?1,
                   message=COALESCE(message,'应用异常退出，阶段未正常结束')
                 WHERE status IN ('pending','running') AND run_id IN (SELECT run_id FROM task_runs WHERE status='failed')",
                params![now_s],
            )
            .map_err(db_error)?;
        Ok(n)
    }

    /// 保留最近 max_keep 次或 max_days 天内的运行
    pub fn purge_old_task_runs(&mut self, max_keep: usize, max_days: i64) -> Result<usize> {
        let cutoff = (Utc::now() - chrono::Duration::days(max_days.max(1))).to_rfc3339();
        let keep = max_keep.max(1) as i64;
        let n = self
            .conn
            .execute(
                "DELETE FROM task_runs WHERE run_id NOT IN (
                    SELECT run_id FROM task_runs ORDER BY started_at DESC LIMIT ?1
                 ) AND started_at < ?2",
                params![keep, cutoff],
            )
            .map_err(db_error)?;
        Ok(n)
    }
}

/// 领域对象转换辅助：列表行 → TaskRun
pub fn task_run_from_row(row: &TaskRunRow) -> Result<TaskRun> {
    Ok(TaskRun {
        run_id: row.run_id.clone(),
        kind: match row.kind.as_str() {
            "restore" => TaskRunKind::Restore,
            _ => TaskRunKind::Pipeline,
        },
        trigger_source: row.trigger_source.clone(),
        status: match row.status.as_str() {
            "success" => TaskRunStatus::Success,
            "partial" => TaskRunStatus::Partial,
            "failed" => TaskRunStatus::Failed,
            "cancelled" => TaskRunStatus::Cancelled,
            _ => TaskRunStatus::Running,
        },
        started_at: parse_time(&row.started_at)?,
        finished_at: optional_time(row.finished_at.as_deref())?,
        duration_ms: row.duration_ms,
        webdav_configured: row.webdav_configured,
        summary_json: row.summary_json.clone(),
        error_message: row.error_message.clone(),
        runner_kind: row.runner_kind.clone(),
        runner_pid: row.runner_pid,
        cancel_requested: row.cancel_requested,
        heartbeat_at: optional_time(row.heartbeat_at.as_deref())?,
    })
}

/// 领域对象转换辅助：阶段行 → TaskRunStage
pub fn stage_from_row(row: &TaskRunStageRow) -> Result<TaskRunStage> {
    let stage = match row.stage.as_str() {
        "archive" => TaskRunStageName::Archive,
        "publish" => TaskRunStageName::Publish,
        "pull" => TaskRunStageName::Pull,
        _ => TaskRunStageName::Scan,
    };
    let status = match row.status.as_str() {
        "running" => TaskRunStageStatus::Running,
        "success" => TaskRunStageStatus::Success,
        "failed" => TaskRunStageStatus::Failed,
        "skipped" => TaskRunStageStatus::Skipped,
        _ => TaskRunStageStatus::Pending,
    };
    Ok(TaskRunStage {
        stage_id: row.stage_id.clone(),
        run_id: row.run_id.clone(),
        stage,
        status,
        started_at: optional_time(row.started_at.as_deref())?,
        finished_at: optional_time(row.finished_at.as_deref())?,
        duration_ms: row.duration_ms,
        stats_json: row.stats_json.clone(),
        message: row.message.clone(),
    })
}

/// 领域对象转换辅助：明细行 → TaskRunItem
pub fn item_from_row(row: &TaskRunItemRow) -> Result<TaskRunItem> {
    let stage = match row.stage.as_str() {
        "archive" => TaskRunStageName::Archive,
        "publish" => TaskRunStageName::Publish,
        "pull" => TaskRunStageName::Pull,
        _ => TaskRunStageName::Scan,
    };
    let status = match row.status.as_str() {
        "missing" => TaskRunItemStatus::Missing,
        "decrypt_failed" => TaskRunItemStatus::DecryptFailed,
        "skipped" => TaskRunItemStatus::Skipped,
        _ => TaskRunItemStatus::Failed,
    };
    Ok(TaskRunItem {
        item_id: row.item_id.clone(),
        run_id: row.run_id.clone(),
        stage,
        record_id: row.record_id.clone(),
        object_id: row.object_id.clone(),
        task_id: row.task_id.clone(),
        name: row.name.clone(),
        status,
        error_code: row.error_code.clone(),
        error_message: row.error_message.clone(),
        size: row.size,
        updated_at: parse_time(&row.updated_at)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatvault_core::models::TaskRunKind;

    #[test]
    fn test_task_run_lifecycle() {
        let mut db = Database::open_in_memory().unwrap();
        let run_id = db
            .start_task_run(TaskRunKind::Pipeline, "manual", false, "desktop")
            .unwrap();

        db.start_task_run_stage(&run_id, TaskRunStageName::Scan)
            .unwrap();
        db.finish_task_run_stage(
            &run_id,
            TaskRunStageName::Scan,
            TaskRunStageStatus::Success,
            Some(r#"{"discovered":10,"new":2}"#),
            Some("扫描完成"),
        )
        .unwrap();

        db.start_task_run_stage(&run_id, TaskRunStageName::Archive)
            .unwrap();
        db.add_task_run_item(
            &run_id,
            &NewTaskRunItem {
                stage: "archive",
                record_id: Some("r1"),
                object_id: None,
                task_id: Some("t1"),
                name: "a.pdf",
                status: TaskRunItemStatus::Failed.as_str(),
                error_code: None,
                error_message: Some("网络超时"),
                size: Some(100),
            },
        )
        .unwrap();
        db.finish_task_run_stage(
            &run_id,
            TaskRunStageName::Archive,
            TaskRunStageStatus::Success,
            Some(r#"{"uploaded":1,"failed":1}"#),
            None,
        )
        .unwrap();

        db.finish_task_run(
            &run_id,
            TaskRunStatus::Partial,
            Some(r#"{"failedItems":1}"#),
            None,
        )
        .unwrap();

        let runs = db.list_task_runs(10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "partial");

        let stages = db.list_task_run_stages(&run_id).unwrap();
        assert_eq!(stages.len(), 2);

        let items = db.list_task_run_items(&run_id, None, None, 50).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "a.pdf");

        let counts = db.count_task_run_items_by_status(&run_id).unwrap();
        assert_eq!(counts, vec![("failed".to_string(), 1)]);
    }

    #[test]
    fn test_mark_stale_and_purge() {
        let mut db = Database::open_in_memory().unwrap();
        let run_id = db
            .start_task_run(TaskRunKind::Pipeline, "schedule", true, "cli")
            .unwrap();
        db.start_task_run_stage(&run_id, TaskRunStageName::Scan)
            .unwrap();

        // 新鲜心跳不应被清扫
        let n = db.mark_stale_running_task_runs().unwrap();
        assert_eq!(n, 0);
        let run = db.get_task_run(&run_id).unwrap().unwrap();
        assert_eq!(run.status, "running");

        // 心跳过期后清扫
        db.conn
            .execute(
                "UPDATE task_runs SET heartbeat_at='2000-01-01T00:00:00Z' WHERE run_id=?1",
                params![run_id],
            )
            .unwrap();
        let n = db.mark_stale_running_task_runs().unwrap();
        assert_eq!(n, 1);
        let run = db.get_task_run(&run_id).unwrap().unwrap();
        assert_eq!(run.status, "failed");

        // 保留策略：超过 keep 且早于 cutoff 才删
        let removed = db.purge_old_task_runs(50, 30).unwrap();
        assert_eq!(removed, 0);
        assert_eq!(db.list_task_runs(10).unwrap().len(), 1);
    }

    #[test]
    fn test_cancel_request_and_progress() {
        let mut db = Database::open_in_memory().unwrap();
        let run_id = db
            .start_task_run(TaskRunKind::Pipeline, "schedule", true, "cli")
            .unwrap();
        db.start_task_run_stage(&run_id, TaskRunStageName::Archive)
            .unwrap();
        db.touch_task_run_progress(&run_id, TaskRunStageName::Archive, 3, 10, Some("a.pdf"))
            .unwrap();
        assert!(!db.task_run_cancel_requested(&run_id).unwrap());
        assert!(db.request_cancel_task_run(&run_id).unwrap());
        assert!(db.task_run_cancel_requested(&run_id).unwrap());
        assert!(db.get_active_task_run().unwrap().is_some());
    }
}
