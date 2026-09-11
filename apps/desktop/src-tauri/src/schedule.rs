// ChatVault Windows 计划任务注册
// 使用 schtasks 创建/删除按周期拉起 chatvault-cli scheduled-run 的系统任务

use std::path::{Path, PathBuf};
use std::process::Command;

/// 计划任务名称
pub const TASK_NAME: &str = "ChatVaultScheduledScan";

/// 定位 chatvault-cli 可执行文件
///
/// 查找顺序：当前 exe 同目录 → 系统 PATH
pub fn find_cli_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("chatvault-cli.exe");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    which_in_path("chatvault-cli.exe")
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// 注册 Windows 计划任务
///
/// 输入:
///   - `cli_path`: chatvault-cli.exe 绝对路径
///   - `db_path`: 本地 SQLite 路径
///   - `interval_minutes`: 间隔分钟数
pub fn register_scheduled_task(
    cli_path: &Path,
    db_path: &Path,
    interval_minutes: u32,
) -> Result<(), String> {
    let minutes = interval_minutes.clamp(5, 1440);
    let tr = format!(
        "\"{}\" scheduled-run --db \"{}\"",
        cli_path.display(),
        db_path.display()
    );

    let output = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/TR",
            &tr,
            "/SC",
            "MINUTE",
            "/MO",
            &minutes.to_string(),
            "/F",
        ])
        .output()
        .map_err(|e| format!("调用 schtasks 失败: {}", e))?;

    if !output.status.success() {
        let msg = if !output.stderr.is_empty() {
            String::from_utf8_lossy(&output.stderr)
        } else {
            String::from_utf8_lossy(&output.stdout)
        };
        return Err(format!("注册计划任务失败: {}", msg.trim()));
    }
    Ok(())
}

/// 删除 Windows 计划任务（任务不存在时视为成功）
pub fn unregister_scheduled_task() -> Result<(), String> {
    let output = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .output()
        .map_err(|e| format!("调用 schtasks 失败: {}", e))?;

    // 任务不存在时 schtasks 返回失败，忽略
    if !output.status.success() {
        let msg = if !output.stderr.is_empty() {
            String::from_utf8_lossy(&output.stderr)
        } else {
            String::from_utf8_lossy(&output.stdout)
        };
        let trimmed = msg.trim();
        if trimmed.contains("找不到") || trimmed.to_lowercase().contains("cannot find") {
            return Ok(());
        }
        return Err(format!("删除计划任务失败: {}", trimmed));
    }
    Ok(())
}

/// 查询计划任务是否存在
pub fn scheduled_task_exists() -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
