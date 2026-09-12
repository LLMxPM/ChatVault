// ChatVault 桌面命令：来源账号与来源聊天映射管理。
// 负责查询映射实体、统计关联文件并提交跨设备同步的名称/收藏修改。
use super::{
    SourceAccountDto, SourceConversationDto, UpdateSourceAccountDto, UpdateSourceConversationDto,
};
use crate::state::AppState;
use chatvault_index::{Database, SourceAccountRow, SourceConversationRow};
use tauri::State;

/// 将索引层来源账号行转换为前端契约。
fn account_dto(row: SourceAccountRow) -> SourceAccountDto {
    SourceAccountDto {
        source_type: row.source_type,
        source_account_id: row.source_account_id,
        source_name: row.source_name,
        display_name: row.display_name,
        effective_name: row.effective_name,
        is_favorite: row.is_favorite,
        record_count: row.record_count,
    }
}

/// 将索引层来源聊天行转换为前端契约。
fn conversation_dto(row: SourceConversationRow) -> SourceConversationDto {
    SourceConversationDto {
        source_type: row.source_type,
        source_account_id: row.source_account_id,
        source_conversation_id: row.source_conversation_id,
        source_name: row.source_name,
        display_name: row.display_name,
        effective_name: row.effective_name,
        is_favorite: row.is_favorite,
        record_count: row.record_count,
    }
}

/// 列出当前资料库中已发现的来源账号映射。
#[tauri::command]
pub async fn list_source_accounts(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<SourceAccountDto>, String> {
    let db = state.get_db().map_err(|error| error.to_string())?;
    db.list_source_accounts()
        .map(|rows| rows.into_iter().map(account_dto).collect())
        .map_err(|error| error.to_string())
}

/// 按来源类型和账号筛选来源聊天映射；参数为空表示不限制该条件。
#[tauri::command]
pub async fn list_source_conversations(
    source_type: Option<String>,
    source_account_id: Option<String>,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<SourceConversationDto>, String> {
    let db = state.get_db().map_err(|error| error.to_string())?;
    db.list_source_conversations(source_type.as_deref(), source_account_id.as_deref())
        .map(|rows| rows.into_iter().map(conversation_dto).collect())
        .map_err(|error| error.to_string())
}

/// 更新来源账号的自定义名称和收藏状态，并与 journal 事件原子提交。
#[tauri::command]
pub async fn update_source_account(
    request: UpdateSourceAccountDto,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let device_id = state.device_id().map_err(|error| error.to_string())?;
    let mut db: Database = state.get_db().map_err(|error| error.to_string())?;
    db.update_source_account(
        &device_id,
        &request.source_type,
        &request.source_account_id,
        request.display_name.as_deref(),
        request.is_favorite,
    )
    .map_err(|error| error.to_string())
}

/// 更新来源聊天的自定义名称和收藏状态，并与 journal 事件原子提交。
#[tauri::command]
pub async fn update_source_conversation(
    request: UpdateSourceConversationDto,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let device_id = state.device_id().map_err(|error| error.to_string())?;
    let mut db: Database = state.get_db().map_err(|error| error.to_string())?;
    db.update_source_conversation(
        &device_id,
        &request.source_type,
        &request.source_account_id,
        &request.source_conversation_id,
        request.display_name.as_deref(),
        request.is_favorite,
    )
    .map_err(|error| error.to_string())
}
