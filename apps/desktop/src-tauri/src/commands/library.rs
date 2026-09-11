// ChatVault 桌面命令：library 职责实现与前端错误映射。
use super::*;

/// 根据关键词、分类大类和来源账号多维检索文件记录
///
/// # 输入
/// - `query`: 查询参数
/// - `state`: 应用全局状态
#[tauri::command]
pub async fn search_records(
    query: SearchQueryDto,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<FileRecordViewDto>, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let search_service = SearchService::new(&db);

    let filter = SearchFilter {
        keyword: query.keyword.clone(),
        category: query.category.clone(),
        account_id: query.account_id.clone(),
        limit: query.limit.unwrap_or(300),
        offset: query.offset.unwrap_or(0),
        ..Default::default()
    };

    let items = search_service.search(&filter).map_err(|e| e.to_string())?;
    let mut results = Vec::new();

    for item in items {
        let cat = determine_category(&item.original_name);

        let formatted_size = format_file_size(item.size);
        let hash = item.object_id.trim_start_matches("blake3:").to_string();

        results.push(FileRecordViewDto {
            record_id: item.record_id,
            object_id: item.object_id,
            original_name: item.original_name,
            file_size: item.size,
            formatted_size,
            hash,
            source_type: item.source,
            account_id: item.account_id,
            file_time: Some(item.file_time),
            discovered_at: item.discovered_at,
            original_path: item.original_path.unwrap_or_default(),
            category: cat,
        });
    }

    Ok(results)
}

/// 获取全局存储统计与去重指标
#[tauri::command]
pub async fn get_vault_stats(
    state: State<'_, AppState>,
) -> std::result::Result<VaultStatsDto, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let conn = db.connection();

    let total_records: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_records", [], |r| r.get(0))
        .unwrap_or(0);

    let unique_objects: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_objects", [], |r| r.get(0))
        .unwrap_or(0);

    let unique_bytes: i64 = conn
        .query_row("SELECT COALESCE(SUM(size), 0) FROM file_objects", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    let total_raw_bytes: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(o.size), 0) FROM file_records r JOIN file_objects o ON r.object_id = o.object_id",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let raw_u64 = total_raw_bytes.max(0) as u64;
    let uniq_u64 = unique_bytes.max(0) as u64;
    let saved_u64 = if raw_u64 >= uniq_u64 {
        raw_u64 - uniq_u64
    } else {
        0
    };

    let ratio = if raw_u64 > 0 {
        (saved_u64 as f64 / raw_u64 as f64) * 100.0
    } else {
        0.0
    };

    Ok(VaultStatsDto {
        total_records,
        unique_objects,
        total_raw_bytes: raw_u64,
        unique_bytes: uniq_u64,
        saved_bytes: saved_u64,
        dedup_ratio_percent: (ratio * 10.0).round() / 10.0,
        formatted_total_raw: format_file_size(raw_u64),
        formatted_saved_bytes: format_file_size(saved_u64),
    })
}

/// 在 Windows 资源管理器中高亮选中文件
#[tauri::command]
pub async fn reveal_file_in_explorer(path: String) -> std::result::Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("文件不存在: {}", path));
    }

    Command::new("explorer")
        .arg(format!("/select,{}", path))
        .spawn()
        .map_err(|e| format!("唤起文件资源管理器失败: {}", e))?;

    Ok(())
}
