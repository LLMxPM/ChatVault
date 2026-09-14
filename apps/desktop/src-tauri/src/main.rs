// ChatVault 桌面端原生服务入口
// 初始化 Tauri 2 运行时、全局状态与 Command 处理程序

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod connection;
mod runtime;
mod schedule;
mod state;

use state::AppState;
use tauri::Manager;

/// 初始化用户资料库与桌面服务；数据路径不依赖快捷方式的工作目录。
fn main() {
    if let Err(error) = run() {
        runtime::report_startup_error(&error.to_string());
        std::process::exit(1);
    }
}

/// 在进入事件循环前完成资料库初始化，让启动错误返回主入口统一报告。
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            commands::scan::detect_wechat_accounts,
            commands::scan::inspect_wechat_directory,
            commands::scan::detect_wxwork_accounts,
            commands::scan::inspect_wxwork_directory,
            commands::scan::run_scan,
            commands::pipeline::run_pipeline,
            commands::pipeline::list_task_runs,
            commands::pipeline::get_task_run_detail,
            commands::pipeline::get_active_run,
            commands::pipeline::cancel_task_run,
            commands::library::search_objects,
            commands::library::list_object_sources,
            commands::library::get_vault_stats,
            commands::library::reveal_file_in_explorer,
            commands::library::open_file_with_system,
            commands::library::download_object,
            commands::library::download_objects,
            commands::library::release_object_cache,
            commands::library::delete_object_local_files,
            commands::library::reclaim_cache_now,
            commands::sources::list_source_accounts,
            commands::sources::list_source_conversations,
            commands::sources::update_source_account,
            commands::sources::update_source_conversation,
            commands::webdav::test_webdav,
            commands::webdav::archive_to_webdav,
            commands::webdav::save_webdav_credential,
            commands::webdav::load_webdav_credential,
            commands::webdav::clear_webdav_credential,
            commands::settings::get_app_settings,
            commands::settings::set_app_settings,
            commands::settings::reset_vault_binding,
            commands::settings::set_collect_sources,
            commands::settings::get_collect_source_cache,
            commands::settings::set_collect_source_cache,
            commands::settings::get_collect_selected_accounts,
            commands::settings::set_collect_selected_accounts,
            commands::settings::set_schedule_config,
            commands::settings::get_schedule_status,
            commands::settings::pick_directory,
            commands::settings::check_directory,
            commands::sync::sync_publish,
            commands::sync::sync_pull,
            commands::sync::sync_restore,
            commands::tasks::list_upload_tasks,
            commands::tasks::requeue_upload_task,
            commands::tasks::pause_upload_task,
            connection::save_webdav_config,
            runtime::get_runtime_info,
            runtime::open_data_directory,
            runtime::open_log_directory,
            runtime::open_repository_homepage,
        ])
        .build(tauri::generate_context!())?;
    let data_dir = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    runtime::init_logging(&data_dir)?;
    let state = AppState::new(data_dir.join("chatvault.db"), state::default_vault_id())?;
    if let Err(error) = runtime::restore_schedule(&state) {
        tracing::warn!("恢复定时任务失败，请在设置中重新启用：{error}");
    }
    app.manage(state);
    app.run(|_, _| {});
    Ok(())
}
