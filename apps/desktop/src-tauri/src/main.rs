// ChatVault 桌面端原生服务入口
// 初始化 Tauri 2 运行时、全局状态与 Command 处理程序

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod connection;
mod schedule;
mod state;

use state::AppState;
use std::path::PathBuf;

fn main() {
    // 默认数据库存放在运行目录或用户主目录下
    let db_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("chatvault.db");

    let app_state = AppState::new(
        db_path,
        "default-vault".to_string(),
        uuid::Uuid::new_v4().to_string(),
    )
    .expect("初始化 ChatVault 核心数据库失败");

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::scan::detect_wechat_accounts,
            commands::scan::run_scan,
            commands::library::search_records,
            commands::library::get_vault_stats,
            commands::library::reveal_file_in_explorer,
            commands::webdav::test_webdav,
            commands::webdav::archive_to_webdav,
            commands::webdav::save_webdav_credential,
            commands::webdav::load_webdav_credential,
            commands::settings::get_app_settings,
            commands::settings::set_app_settings,
            commands::settings::get_schedule_status,
            commands::sync::sync_publish,
            commands::sync::sync_pull,
            commands::sync::sync_restore,
            commands::tasks::list_upload_tasks,
            commands::tasks::requeue_upload_task,
            commands::tasks::pause_upload_task,
            connection::save_webdav_config,
            connection::list_record_accounts,
        ])
        .run(tauri::generate_context!())
        .expect("启动 ChatVault 桌面端应用失败");
}
