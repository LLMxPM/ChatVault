// ChatVault 桌面命令：tasks 职责实现与前端错误映射。
use super::*;

/// 上传任务列表项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadTaskDto {
    pub task_id: String,
    pub status: String,
    pub retry_count: u32,
    pub error_message: Option<String>,
    pub updated_at: String,
    pub original_name: String,
    pub record_id: String,
    pub hash: String,
    pub size: u64,
    pub formatted_size: String,
    pub original_path: Option<String>,
}

/// 列出上传任务
#[tauri::command]
pub async fn list_upload_tasks(
    status: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<UploadTaskDto>, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let rows = db
        .list_upload_tasks(status.as_deref(), limit.unwrap_or(200))
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let formatted_size = format_file_size(r.size);
            UploadTaskDto {
                task_id: r.task_id,
                status: r.status,
                retry_count: r.retry_count,
                error_message: r.error_message,
                updated_at: r.updated_at,
                original_name: r.original_name,
                record_id: r.record_id,
                hash: r.hash,
                size: r.size,
                formatted_size,
                original_path: r.original_path,
            }
        })
        .collect())
}

/// 重新入队任务
#[tauri::command]
pub async fn requeue_upload_task(
    task_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    db.requeue_task(&task_id).map_err(|e| e.to_string())
}

/// 暂停任务
#[tauri::command]
pub async fn pause_upload_task(
    task_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    db.pause_task(&task_id).map_err(|e| e.to_string())
}

/// 删除本地缺失的上传队列项；拒绝已删除或状态已变化的任务。
#[tauri::command]
pub async fn delete_missing_upload_task(
    task_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    if db
        .delete_missing_upload_task(&task_id)
        .map_err(|e| e.to_string())?
    {
        Ok(())
    } else {
        Err("任务已不存在或不再是本地缺失状态，请刷新队列后重试".into())
    }
}
