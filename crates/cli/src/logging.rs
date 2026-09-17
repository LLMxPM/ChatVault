// CLI 日志：定时运行按日落盘并保留七天，其他命令仍向终端输出。
use crate::args::Commands;
use anyhow::{Context, Result};
use chrono::{Days, NaiveDate, Utc};
use std::{fs::OpenOptions, path::Path, sync::Mutex};

/// 根据已解析命令初始化日志；定时运行忽略外部日志过滤，保证关键执行信息可回看。
pub(crate) fn init(command: &Commands) -> Result<()> {
    if let Commands::ScheduledRun { db } = command {
        let db = std::path::absolute(db)?;
        let directory = db.parent().context("数据库路径没有父目录")?.join("logs");
        std::fs::create_dir_all(&directory).context("创建定时日志目录失败")?;
        let today = Utc::now().date_naive();
        let path = directory.join(format!("scheduled-{today}.log"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .context("打开定时日志文件失败")?;
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::INFO)
            .with_writer(Mutex::new(file))
            .try_init()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        if let Err(error) = prune_logs(&directory, today) {
            tracing::warn!(%error, "清理过期定时日志失败");
        }
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            tracing::error!("CLI 异常：{info}");
            default_hook(info);
        }));
        tracing::info!(log = %path.display(), pid = std::process::id(), "定时日志已初始化");
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .init();
    }
    Ok(())
}

/// 仅删除本目录中名称严格匹配且超过七个 UTC 日期的定时日志，不触碰其他文件。
fn prune_logs(directory: &Path, today: NaiveDate) -> Result<()> {
    let cutoff = today
        .checked_sub_days(Days::new(6))
        .context("日志日期越界")?;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(date) = name
            .strip_prefix("scheduled-")
            .and_then(|name| name.strip_suffix(".log"))
        else {
            continue;
        };
        let Ok(parsed) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
            continue;
        };
        if parsed.to_string() == date && parsed < cutoff {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
