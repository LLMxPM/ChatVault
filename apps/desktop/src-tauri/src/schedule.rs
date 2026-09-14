// ChatVault Windows 计划任务注册
// 用 schtasks 注册，任务动作经 PowerShell -WindowStyle Hidden 拉起 CLI，避免控制台弹窗。
// 删除/查询仍用 schtasks，与安装生命周期脚本约定一致。

use std::path::{Path, PathBuf};
use std::process::Command;

/// 计划任务名称
pub const TASK_NAME: &str = "ChatVaultScheduledScan";

/// 计划任务允许的最大执行间隔：7 天。
pub const MAX_INTERVAL_MINUTES: u32 = 7 * 24 * 60;

/// 定位 chatvault-cli 可执行文件
///
/// 只使用同版本安装包或构建目录中的 CLI，避免调用 PATH 中的其他程序。
pub fn find_cli_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("chatvault-cli.exe");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 将路径转成可在 PowerShell 单引号字符串中安全嵌入的字面量。
fn ps_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

/// 注册 Windows 计划任务
///
/// 输入:
///   - `cli_path`: chatvault-cli.exe 绝对路径
///   - `db_path`: 本地 SQLite 路径
///   - `interval_minutes`: 间隔分钟数
///
/// 任务动作经 `powershell -WindowStyle Hidden` 启动 CLI：计划任务拉起控制台程序时
/// 不再直接弹出黑色窗口；PowerShell 仍以当前用户交互令牌运行，可读用户凭据与目录。
pub fn register_scheduled_task(
    cli_path: &Path,
    db_path: &Path,
    interval_minutes: u32,
) -> Result<(), String> {
    let minutes = interval_minutes.clamp(5, MAX_INTERVAL_MINUTES);
    let cli = ps_single_quote(&cli_path.display().to_string());
    let db = ps_single_quote(&db_path.display().to_string());
    let tr = format!(
        "powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -Command \"& '{cli}' scheduled-run --db '{db}'\""
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
