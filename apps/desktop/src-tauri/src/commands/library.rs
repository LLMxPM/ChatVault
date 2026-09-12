// ChatVault 桌面命令：library 职责实现与前端错误映射。
use super::webdav::resolve_webdav_password;
use super::*;
use chatvault_index::query::{ObjectSearchItem, ObjectSourceItem};
use chatvault_metadata::get_object_path;

/// 内容对象列表项 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileObjectViewDto {
    pub object_id: String,
    pub hash: String,
    pub original_name: String,
    pub file_size: u64,
    pub formatted_size: String,
    pub category: String,
    pub file_time: Option<String>,
    pub location: String,
    pub open_path: Option<String>,
    pub source_count: usize,
}

/// 内容对象来源项 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSourceDto {
    pub record_id: String,
    pub original_name: String,
    pub original_path: String,
    pub source_type: String,
    pub source_account_id: Option<String>,
    pub source_account_name: Option<String>,
    pub source_conversation_id: Option<String>,
    pub source_conversation_name: Option<String>,
    pub file_time: Option<String>,
    pub discovered_at: String,
    pub device_id: String,
    pub has_local_path: bool,
}

/// 下载结果 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResultDto {
    pub saved_path: String,
    pub file_name: String,
    pub size: u64,
}

fn map_object_item(item: ObjectSearchItem) -> FileObjectViewDto {
    let cat = determine_category(&item.original_name);
    FileObjectViewDto {
        object_id: item.object_id,
        hash: item.hash,
        original_name: item.original_name,
        file_size: item.size,
        formatted_size: format_file_size(item.size),
        category: cat,
        file_time: Some(item.file_time),
        location: item.location.as_str().to_string(),
        open_path: item.open_path,
        source_count: item.source_count,
    }
}

/// 按内容对象折叠检索文件库
#[tauri::command]
pub async fn search_objects(
    query: SearchQueryDto,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<FileObjectViewDto>, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let search_service = SearchService::new(&db);

    let filter = SearchFilter {
        keyword: query.keyword.clone(),
        category: query.category.clone(),
        source_type: query.source_type.clone(),
        source_account_id: query.source_account_id.clone(),
        source_conversation_id: query.source_conversation_id.clone(),
        limit: query.limit.unwrap_or(300),
        offset: query.offset.unwrap_or(0),
        ..Default::default()
    };

    let items = search_service
        .search_objects(&filter, &device_id)
        .map_err(|e| e.to_string())?;
    Ok(items.into_iter().map(map_object_item).collect())
}

/// 列出内容对象的全部来源
#[tauri::command]
pub async fn list_object_sources(
    object_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<FileSourceDto>, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let search_service = SearchService::new(&db);
    let items = search_service
        .list_object_sources(&object_id)
        .map_err(|e| e.to_string())?;
    Ok(items
        .into_iter()
        .map(|s: ObjectSourceItem| FileSourceDto {
            record_id: s.record_id,
            original_name: s.original_name,
            original_path: s.original_path.unwrap_or_default(),
            source_type: s.source_type,
            source_account_id: s.source_account_id,
            source_account_name: s.source_account_name,
            source_conversation_id: s.source_conversation_id,
            source_conversation_name: s.source_conversation_name,
            file_time: Some(s.file_time),
            discovered_at: s.discovered_at,
            device_id: s.device_id,
            has_local_path: s.has_local_path,
        })
        .collect())
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

/// 用系统默认程序打开本地文件
///
/// 使用 explorer.exe 直接打开文件（不经 cmd 解析），避免路径元字符被 shell 解释。
#[tauri::command]
pub async fn open_file_with_system(path: String) -> std::result::Result<(), String> {
    let p = Path::new(&path);
    if !p.is_file() {
        return Err(format!("文件不存在或不可读: {}", path));
    }

    Command::new("explorer")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("打开文件失败: {}", e))?;

    Ok(())
}

/// 解析下载目录：空则使用用户 Downloads/ChatVault
pub(crate) fn resolve_download_dir(configured: &str) -> PathBuf {
    let trimmed = configured.trim();
    if !trimmed.is_empty() {
        return PathBuf::from(trimmed);
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return PathBuf::from(profile).join("Downloads").join("ChatVault");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join("Downloads").join("ChatVault");
    }
    std::env::temp_dir().join("ChatVault-Downloads")
}

/// 生成不冲突的目标文件名
fn unique_dest_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    for i in 1..1000 {
        let name = if ext.is_empty() {
            format!("{stem} ({i})")
        } else {
            format!("{stem} ({i}).{ext}")
        };
        let p = dir.join(&name);
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!(
        "{stem}-{}.{}",
        std::process::id(),
        if ext.is_empty() { "bin" } else { ext }
    ))
}

/// 从 WebDAV 下载单个内容对象到用户下载目录
#[tauri::command]
pub async fn download_object(
    object_id: String,
    original_name: String,
    state: State<'_, AppState>,
) -> std::result::Result<DownloadResultDto, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let vault_id = state.vault_id().map_err(|e| e.to_string())?;

    let webdav_url = db
        .get_setting(setting_keys::WEBDAV_URL)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let webdav_username = db
        .get_setting(setting_keys::WEBDAV_USERNAME)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let download_dir_cfg = db
        .get_setting(setting_keys::DOWNLOAD_DIR)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    if webdav_url.trim().is_empty() {
        return Err("尚未配置 WebDAV，请先在设置中完成连接配置".into());
    }

    let hash: String = {
        let conn = db.connection();
        conn.query_row(
            "SELECT hash FROM file_objects WHERE object_id = ?1",
            [&object_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("未找到内容对象: {e}"))?
    };

    let password = resolve_webdav_password(&webdav_url, &webdav_username, None);
    if password.is_none() {
        return Err("尚未保存 WebDAV 密码，请先在设置中完成连接配置".into());
    }
    let client = WebDavClient::new(WebDavConfig {
        base_url: webdav_url,
        username: Some(webdav_username),
        password,
    })
    .map_err(|e| e.to_string())?;

    let remote_path = get_object_path(&vault_id, &hash);
    let dir = resolve_download_dir(&download_dir_cfg);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("无法创建下载目录 {}: {e}", dir.display()))?;

    let safe_name = if original_name.trim().is_empty() {
        format!("{}.bin", hash.chars().take(16).collect::<String>())
    } else {
        Path::new(&original_name)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| original_name.clone())
    };
    let dest = unique_dest_path(&dir, &safe_name);

    let size = client
        .download_to_path(&remote_path, &dest, &hash)
        .await
        .map_err(|e| e.to_string())?;

    let saved_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| safe_name.clone());

    Ok(DownloadResultDto {
        saved_path: dest.to_string_lossy().into_owned(),
        file_name: saved_name,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_download_dir_prefers_configured_path() {
        let dir = resolve_download_dir("D:\\MyDl");
        assert_eq!(dir, PathBuf::from("D:\\MyDl"));
    }

    #[test]
    fn resolve_download_dir_default_under_user_downloads() {
        let dir = resolve_download_dir("");
        let s = dir.to_string_lossy();
        assert!(s.contains("Downloads"));
        assert!(s.contains("ChatVault"));
    }

    #[test]
    fn unique_dest_path_appends_suffix_on_conflict() {
        let tmp = std::env::temp_dir().join(format!("chatvault-dl-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("a.txt"), b"1").unwrap();
        let p = unique_dest_path(&tmp, "a.txt");
        assert_eq!(p.file_name().unwrap(), "a (1).txt");
        std::fs::write(&p, b"2").unwrap();
        let p2 = unique_dest_path(&tmp, "a.txt");
        assert_eq!(p2.file_name().unwrap(), "a (2).txt");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
