// 定时运行收尾：由阶段统计构造与桌面一致的摘要，持久化成功、取消与异常。
use anyhow::Result;
use chatvault_core::models::{TaskRunStageName, TaskRunStageStatus, TaskRunStatus};
use chatvault_index::Database;
use chatvault_scan::ScanReport;
use chatvault_sync::ArchiveReport;
use serde_json::{json, Value};

/// 使用新增内容对象数，而非来源记录数，保证去重后的新文件计数准确。
pub(super) fn scan_stats(report: &ScanReport) -> Value {
    json!({ "discovered": report.discovered, "newObjects": report.new_objects, "skipped": report.skipped })
}

/// 归档阶段统计与桌面立即运行使用相同字段。
pub(super) fn archive_stats(report: &ArchiveReport) -> Value {
    json!({ "uploaded": report.uploaded, "verified": report.verified, "failed": report.failed, "processed": report.processed })
}

/// 按运行明细计算异常数量，与桌面列表的统计口径一致。
pub(super) fn failed_items(db: &Database, run_id: &str) -> Result<i64> {
    Ok(db
        .count_task_run_items_by_status(run_id)?
        .iter()
        .filter(|(status, _)| status != "skipped")
        .map(|(_, count)| *count)
        .sum())
}

/// 所有结束分支共用当前摘要结构，不读取或兼容历史平铺格式。
pub(super) fn finish_run(
    db: &mut Database,
    run_id: &str,
    status: TaskRunStatus,
    error: Option<&str>,
) -> Result<()> {
    let mut summary =
        json!({ "scan": null, "archive": null, "failedItems": failed_items(db, run_id)? });
    for stage in db.list_task_run_stages(run_id)? {
        let Some(raw) = stage.stats_json else {
            continue;
        };
        let stats: Value = serde_json::from_str(&raw)?;
        match stage.stage.as_str() {
            "scan" | "archive" => summary[&stage.stage] = stats,
            "publish" => summary["publishedSeq"] = stats["publishedSeq"].clone(),
            "pull" => summary["applied"] = stats["applied"].clone(),
            _ => {}
        }
    }
    if status == TaskRunStatus::Cancelled {
        summary["cancelled"] = json!(true);
    }
    let summary = summary.to_string();
    db.finish_task_run(run_id, status, Some(&summary), error)?;
    tracing::info!(status = status.as_str(), %summary, "定时运行结束");
    Ok(())
}

/// 取消时保留实际阶段统计；未开始的阶段不伪造执行结果。
pub(super) fn finish_cancelled(
    db: &mut Database,
    run_id: &str,
    stage: TaskRunStageName,
    stats: &Value,
) -> Result<()> {
    db.finish_task_run_stage(
        run_id,
        stage,
        TaskRunStageStatus::Skipped,
        Some(&stats.to_string()),
        Some("用户取消"),
    )?;
    finish_run(db, run_id, TaskRunStatus::Cancelled, Some("用户取消"))
}

/// 收尾尚未结束的运行；同步错误保留 partial，已经写入的终态不会被覆盖。
pub(super) fn finish_error(db: &mut Database, run_id: &str, error: &anyhow::Error) -> Result<()> {
    let Some(run) = db.get_task_run(run_id)? else {
        return Ok(());
    };
    if run.status != "running" {
        return Ok(());
    }
    let message = format!("{error:#}");
    let mut status = TaskRunStatus::Failed;
    for stage in db.list_task_run_stages(run_id)? {
        if stage.status != "running" {
            continue;
        }
        let name = match stage.stage.as_str() {
            "scan" => TaskRunStageName::Scan,
            "archive" => TaskRunStageName::Archive,
            "publish" => {
                status = TaskRunStatus::Partial;
                TaskRunStageName::Publish
            }
            "pull" => {
                status = TaskRunStatus::Partial;
                TaskRunStageName::Pull
            }
            _ => continue,
        };
        db.finish_task_run_stage(
            run_id,
            name,
            TaskRunStageStatus::Failed,
            stage.stats_json.as_deref(),
            Some(&message),
        )?;
    }
    finish_run(db, run_id, status, Some(&message))
}

#[cfg(test)]
#[path = "reporting_tests.rs"]
mod tests;
