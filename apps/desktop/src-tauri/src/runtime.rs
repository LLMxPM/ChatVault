// 拾文桌面运行信息与日志：提供固定数据目录、版本、首次使用状态和日志入口。

use crate::state::AppState;
use serde::Serialize;
use std::{fs::OpenOptions, path::Path};
use tauri::State;

/// 覆盖安装或重装后的首次启动按已保存设置重建系统任务，修正 CLI 安装路径。
pub fn restore_schedule(state: &AppState) -> Result<(), String> {
    let db = state.get_db().map_err(|error| error.to_string())?;
    if db
        .get_setting("schedule_enabled")
        .map_err(|error| error.to_string())?
        .as_deref()
        == Some("true")
    {
        let interval = db
            .get_setting("scan_interval_minutes")
            .map_err(|error| error.to_string())?
            .and_then(|value| value.parse().ok())
            .unwrap_or(30);
        let cli =
            crate::schedule::find_cli_path().ok_or("未找到同目录归档程序，请重新安装完整安装包")?;
        crate::schedule::register_scheduled_task(&cli, &state.db_path, interval)?;
    }
    Ok(())
}

/// 无控制台的 Windows 发行程序也必须显示初始化失败原因。
pub fn report_startup_error(error: &str) {
    eprintln!("拾文启动失败：{error}");
    tracing::error!("启动失败：{error}");
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        extern "system" {
            fn MessageBoxW(window: isize, text: *const u16, caption: *const u16, kind: u32) -> i32;
        }
        let text: Vec<u16> =
            format!("拾文启动失败：{error}\n请查看用户数据目录中 logs 目录的日志。")
                .encode_utf16()
                .chain(Some(0))
                .collect();
        let caption: Vec<u16> = "拾文 ChatVault".encode_utf16().chain(Some(0)).collect();
        // 两个 UTF-16 缓冲区均以零结尾，且在同步系统调用结束前保持有效。
        unsafe {
            MessageBoxW(0, text.as_ptr(), caption.as_ptr(), 0x10);
        }
    }
}

/// 初始化按启动轮换的文件日志；仅保留本次和上次启动日志。
pub fn init_logging(data_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = data_dir.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let current = log_dir.join("desktop.log");
    let previous = log_dir.join("desktop.previous.log");
    if current.exists() {
        if previous.exists() {
            std::fs::remove_file(&previous)?;
        }
        std::fs::rename(&current, previous)?;
    }
    let file = OpenOptions::new().create(true).append(true).open(current)?;
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::sync::Mutex::new(file))
        .try_init()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("桌面程序异常：{info}");
        // 保留默认输出，让开发终端和 RUST_BACKTRACE 仍能显示异常及调用栈。
        default_hook(info);
    }));
    tracing::info!("拾文桌面服务启动");
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    version: String,
    data_directory: String,
    first_run: bool,
}

/// 返回发行版本、数据目录，以及尚无文件记录的首次使用状态。
#[tauri::command]
pub fn get_runtime_info(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<RuntimeInfo, String> {
    let db = state.get_db().map_err(|error| error.to_string())?;
    let count = db
        .get_stats()
        .map_err(|error| error.to_string())?
        .total_records;
    Ok(RuntimeInfo {
        version: app.package_info().version.to_string(),
        data_directory: state
            .db_path
            .parent()
            .ok_or("无效的数据目录")?
            .display()
            .to_string(),
        first_run: count == 0,
    })
}

/// 打开本应用数据目录，不接受前端提供的任意路径。
#[tauri::command]
pub fn open_data_directory(state: State<'_, AppState>) -> Result<(), String> {
    let directory = state.db_path.parent().ok_or("无效的数据目录")?;
    std::process::Command::new("explorer.exe")
        .arg(directory)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// 打开本应用日志目录，不接受前端提供的任意路径。
#[tauri::command]
pub fn open_log_directory(state: State<'_, AppState>) -> Result<(), String> {
    let directory = state.db_path.parent().ok_or("无效的数据目录")?.join("logs");
    std::process::Command::new("explorer.exe")
        .arg(directory)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// 打开项目开源仓库，方便用户查阅文档或 star；地址固定，不接受前端传入 URL。
#[tauri::command]
pub fn open_repository_homepage() -> Result<(), String> {
    const REPOSITORY_URL: &str = "https://github.com/LLMxPM/ChatVault";
    std::process::Command::new("explorer.exe")
        .arg(REPOSITORY_URL)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}
