// 定时采集：读取已保存的采集源与账号选择，复用共享扫描服务。
use anyhow::{Context, Result};
use chatvault_core::models::{CollectSource, TaskRunStageName};
use chatvault_index::Database;
use chatvault_scan::{AccountTarget, ScanReport, ScanRequest};
use chrono::Utc;

/// 读取当前配置并扫描；来源边界观察取消，返回已完成部分的真实计数。
pub(super) fn collect(db: &mut Database, run_id: &str, device_id: &str) -> Result<ScanReport> {
    let sources: Vec<CollectSource> = serde_json::from_str(
        &db.get_setting("collect_sources")?
            .unwrap_or_else(|| "[]".to_string()),
    )
    .context("解析采集源配置失败")?;
    let selected_accounts = load_selected_accounts(db)?;
    let on_event = |event| crate::sync_commands::print_scan_event(&event);
    let request = ScanRequest {
        device_id,
        full_scan: false,
        target_accounts: selected_accounts.as_deref(),
        scan_started_ms: Utc::now().timestamp_millis(),
        should_cancel: None,
        on_event: Some(&on_event),
        on_source_done: None,
    };
    let mut report = ScanReport::default();
    for source in sources {
        if db.task_run_cancel_requested(run_id)? {
            break;
        }
        report.merge(chatvault_scan::scan_collect_sources(
            db,
            std::slice::from_ref(&source),
            &request,
        )?);
        tracing::info!(source = %source.path, discovered = report.discovered, new_objects = report.new_objects, "采集源完成");
        db.touch_task_run_progress(
            run_id,
            TaskRunStageName::Scan,
            report.indexed,
            report.discovered.max(report.indexed),
            Some(&source.path),
        )?;
    }
    Ok(report)
}

/// 账号选择与前端字段保持一致；缺失或 null 表示尚未配置，空数组表示全不选。
fn load_selected_accounts(db: &Database) -> Result<Option<Vec<AccountTarget>>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SelectedAccount {
        source_root: String,
        source_account_id: String,
    }
    let Some(raw) = db.get_setting("collect_selected_accounts")? else {
        return Ok(None);
    };
    let accounts: Option<Vec<SelectedAccount>> =
        serde_json::from_str(&raw).context("解析账号勾选配置失败")?;
    Ok(accounts.map(|accounts| {
        accounts
            .into_iter()
            .map(|account| AccountTarget {
                source_root: account.source_root,
                source_account_id: account.source_account_id,
            })
            .collect()
    }))
}
