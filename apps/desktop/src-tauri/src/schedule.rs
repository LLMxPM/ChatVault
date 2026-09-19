// ChatVault Windows 计划任务注册
// 用 schtasks 注册；任务动作经同目录 VBS 启动器拉起 CLI，避免控制台弹窗闪烁。
// 删除/查询仍用 schtasks，与安装生命周期脚本约定一致。

use std::path::{Path, PathBuf};
use std::process::Command;

/// 计划任务名称
pub const TASK_NAME: &str = "ChatVaultScheduledScan";

/// 计划任务允许的最大执行间隔：7 天。
pub const MAX_INTERVAL_MINUTES: u32 = 7 * 24 * 60;

/// 与 CLI 同目录的 VBS 启动器文件名；安装清理脚本按此路径识别任务归属。
pub const LAUNCHER_VBS_NAME: &str = "chatvault-scheduled-run.vbs";

/// GUI 子系统下拉起 schtasks 等控制台程序时禁止新建控制台窗口。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 构建不弹控制台窗口的 schtasks 命令。
fn schtasks_command() -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("schtasks");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

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

/// 启动器 VBS 与 CLI 同目录，便于安装脚本按安装目录识别并清理。
fn launcher_vbs_path(cli_path: &Path) -> PathBuf {
    cli_path
        .parent()
        .map(|dir| dir.join(LAUNCHER_VBS_NAME))
        .unwrap_or_else(|| PathBuf::from(LAUNCHER_VBS_NAME))
}

/// 生成零窗口启动 CLI 的 VBS 源码。
///
/// wscript 为 GUI 子系统，Run 的窗口样式 0 表示隐藏；比 schtasks 直接拉
/// 控制台程序或 PowerShell -WindowStyle Hidden 更不易闪黑框。
fn launcher_vbs_content(cli_path: &Path, db_path: &Path) -> String {
    let command = format!(
        "\"{}\" scheduled-run --db \"{}\"",
        cli_path.display(),
        db_path.display()
    );
    // VBS 字符串字面量中双引号需写成两个双引号，外层再包一对引号。
    let command_literal = format!("\"{}\"", command.replace('"', "\"\""));
    format!(
        "Option Explicit\r\n\
         Dim shell\r\n\
         Set shell = CreateObject(\"WScript.Shell\")\r\n\
         shell.Run {}, 0, False\r\n",
        command_literal
    )
}

/// 将 VBS 文本写成 UTF-16 LE（带 BOM），保证中文路径可被 wscript 正确读取。
fn write_launcher_vbs(vbs_path: &Path, content: &str) -> Result<(), String> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in content.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(vbs_path, bytes)
        .map_err(|e| format!("写入定时启动脚本失败（{}）: {}", vbs_path.display(), e))
}

/// 注册 Windows 计划任务
///
/// 输入:
///   - `cli_path`: chatvault-cli.exe 绝对路径
///   - `db_path`: 本地 SQLite 路径
///   - `interval_minutes`: 间隔分钟数
///
/// 先在 CLI 同目录写出 VBS 启动器，再用 `wscript //B //Nologo` 注册任务：
/// 计划任务不再直接拉起控制台程序，也不会经 PowerShell Hidden 包装，避免每几分钟闪一次黑框。
/// wscript 仍以当前用户交互令牌运行，可读用户凭据与目录。
pub fn register_scheduled_task(
    cli_path: &Path,
    db_path: &Path,
    interval_minutes: u32,
) -> Result<(), String> {
    let minutes = interval_minutes.clamp(5, MAX_INTERVAL_MINUTES);
    let vbs_path = launcher_vbs_path(cli_path);
    write_launcher_vbs(&vbs_path, &launcher_vbs_content(cli_path, db_path))?;

    // schtasks 存储完整命令行；路径含空格/中文时对 VBS 路径加双引号。
    let tr = format!("wscript.exe //B //Nologo \"{}\"", vbs_path.display());

    let output = schtasks_command()
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

/// 删除 Windows 计划任务（任务不存在时视为成功）；并尽力移除同目录 VBS 启动器。
pub fn unregister_scheduled_task() -> Result<(), String> {
    let output = schtasks_command()
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
        if !trimmed.contains("找不到") && !trimmed.to_lowercase().contains("cannot find") {
            return Err(format!("删除计划任务失败: {}", trimmed));
        }
    }

    if let Some(cli) = find_cli_path() {
        let _ = std::fs::remove_file(launcher_vbs_path(&cli));
    }
    Ok(())
}

/// 查询计划任务是否存在
pub fn scheduled_task_exists() -> bool {
    schtasks_command()
        .args(["/Query", "/TN", TASK_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_vbs_sits_next_to_cli() {
        let cli = Path::new(r"C:\Test Installation\拾文\chatvault-cli.exe");
        assert_eq!(
            launcher_vbs_path(cli),
            Path::new(r"C:\Test Installation\拾文\chatvault-scheduled-run.vbs")
        );
    }

    #[test]
    fn launcher_vbs_embeds_quoted_command() {
        let content = launcher_vbs_content(
            Path::new(r"C:\Test Installation\拾文\chatvault-cli.exe"),
            Path::new(r"C:\Users\李立国\AppData\Local\com.chatvault.desktop\chatvault.db"),
        );
        assert!(content.contains("CreateObject(\"WScript.Shell\")"));
        assert!(content.contains("shell.Run "));
        assert!(content.contains(", 0, False"));
        // 路径两侧应生成转义后的引号，整体命令作为 VBS 字符串传入 Run。
        assert!(
            content.contains(
                "\"\"\"C:\\Test Installation\\拾文\\chatvault-cli.exe\"\" scheduled-run --db \"\"C:\\Users\\李立国\\AppData\\Local\\com.chatvault.desktop\\chatvault.db\"\"\""
            )
        );
    }
}
