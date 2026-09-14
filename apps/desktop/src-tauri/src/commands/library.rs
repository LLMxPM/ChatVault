// ChatVault 桌面命令：library 职责实现与前端错误映射。
use super::webdav::resolve_webdav_password;
use super::*;
use chatvault_index::query::{ObjectSearchItem, ObjectSort, ObjectSourceItem, TimeField};
use chatvault_metadata::get_object_path;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

/// 内容对象列表项 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileObjectViewDto {
    pub object_id: String,
    pub hash: String,
    pub original_name: String,
    pub extension: String,
    pub file_size: u64,
    pub formatted_size: String,
    pub category: String,
    pub file_time: Option<String>,
    pub discovered_at: Option<String>,
    pub time_source: Option<String>,
    pub location: String,
    pub open_path: Option<String>,
    pub source_count: usize,
}

/// 对象检索分页信封
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectSearchPageDto {
    pub total: usize,
    pub items: Vec<FileObjectViewDto>,
}

/// 批量操作单项结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItemResultDto {
    pub object_id: String,
    pub original_name: String,
    pub status: String,
    pub saved_path: Option<String>,
    pub released_bytes: Option<u64>,
    pub error: Option<String>,
}

/// 批量操作汇总
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResultDto {
    pub total: usize,
    pub ok_count: usize,
    pub failed_count: usize,
    pub released_bytes: u64,
    pub items: Vec<BatchItemResultDto>,
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
    /// 设备展示名称；本机优先用设置中的设备名称
    pub device_name: Option<String>,
    /// 是否本机设备
    pub is_local: bool,
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
        extension: item.extension,
        file_size: item.size,
        formatted_size: format_file_size(item.size),
        category: cat,
        file_time: Some(item.file_time),
        discovered_at: Some(item.discovered_at),
        time_source: Some(item.time_source),
        location: item.location.as_str().to_string(),
        open_path: item.open_path,
        source_count: item.source_count,
    }
}

/// 解析前端时间：支持 RFC3339、`YYYY-MM-DD` 与 `YYYY-MM-DDTHH:mm[:ss]`
fn parse_query_time(value: &str, end_of_day: bool) -> Option<DateTime<Utc>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let time = if end_of_day {
            NaiveTime::from_hms_opt(23, 59, 59)?
        } else {
            NaiveTime::from_hms_opt(0, 0, 0)?
        };
        return Some(Utc.from_utc_datetime(&NaiveDateTime::new(date, time)));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Some(Utc.from_utc_datetime(&dt));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M") {
        return Some(Utc.from_utc_datetime(&dt));
    }
    None
}

fn build_search_filter(query: &SearchQueryDto) -> SearchFilter {
    SearchFilter {
        keyword: query.keyword.clone(),
        category: query.category.clone(),
        source_type: query.source_type.clone(),
        source_account_id: query.source_account_id.clone(),
        source_conversation_id: query.source_conversation_id.clone(),
        start_time: query
            .start_time
            .as_deref()
            .and_then(|v| parse_query_time(v, false)),
        end_time: query
            .end_time
            .as_deref()
            .and_then(|v| parse_query_time(v, true)),
        time_field: TimeField::parse(query.time_field.as_deref()),
        location: query.location.clone(),
        hidden_only: query.hidden,
        extensions: query.extensions.clone().unwrap_or_default(),
        sort: ObjectSort::parse(query.sort.as_deref()),
        limit: query.limit.unwrap_or(300),
        offset: query.offset.unwrap_or(0),
        ..Default::default()
    }
}

/// 按内容对象折叠检索文件库
#[tauri::command]
pub async fn search_objects(
    query: SearchQueryDto,
    state: State<'_, AppState>,
) -> std::result::Result<ObjectSearchPageDto, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let search_service = SearchService::new(&db);
    let filter = build_search_filter(&query);
    let page = search_service
        .search_objects(&filter, &device_id)
        .map_err(|e| e.to_string())?;
    Ok(ObjectSearchPageDto {
        total: page.total,
        items: page.items.into_iter().map(map_object_item).collect(),
    })
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
    let local_device_id = state.device_id().map_err(|e| e.to_string())?;
    let local_device_name = state.device_name().map_err(|e| e.to_string())?;
    Ok(items
        .into_iter()
        .map(|s: ObjectSourceItem| {
            let is_local = s.device_id == local_device_id;
            FileSourceDto {
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
                device_name: if is_local {
                    Some(local_device_name.clone())
                } else {
                    s.device_name
                },
                is_local,
                has_local_path: s.has_local_path,
            }
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
    let saved_u64 = raw_u64.saturating_sub(uniq_u64);

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
pub async fn open_file_with_system(
    path: String,
    extension: Option<String>,
) -> std::result::Result<(), String> {
    let p = Path::new(&path);
    let metadata =
        std::fs::symlink_metadata(p).map_err(|_| format!("文件不存在或不可读: {}", path))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!("文件不存在或不可读: {}", path));
    }

    // 哈希缓存没有扩展名时，创建受控的带真实后缀硬链接（跨卷时原子复制），
    // 让 Windows 能按图片/视频类型选择默认程序；源缓存本身仍作为唯一打开依据。
    let open_path = if p.extension().is_none() {
        match extension.as_deref().filter(|value| {
            !value.is_empty()
                && value.len() <= 16
                && value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        }) {
            Some(ext) => prepare_extension_open_path(p, ext)
                .map_err(|error| format!("准备系统打开副本失败: {error}"))?,
            None => p.to_path_buf(),
        }
    } else {
        p.to_path_buf()
    };

    Command::new("explorer")
        .arg(&open_path)
        .spawn()
        .map_err(|e| format!("打开文件失败: {}", e))?;

    Ok(())
}

/// 为无扩展名的受控缓存准备系统打开副本。
fn prepare_extension_open_path(path: &Path, extension: &str) -> std::io::Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "缓存路径缺少目录"))?;
    let open_dir = parent.join("open");
    std::fs::create_dir_all(&open_dir)?;
    let stem = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "缓存文件名无效"))?;
    let target = open_dir.join(format!("{stem}.{extension}"));
    match std::fs::symlink_metadata(&target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "系统打开副本不是普通文件",
                ));
            }
            if chatvault_metadata::verify_file_hash(&target, stem).is_ok() {
                return Ok(target);
            }
            std::fs::remove_file(&target)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    // 同一受控目录优先使用硬链接，避免额外复制明文；跨卷或不支持硬链接时再复制。
    if std::fs::hard_link(path, &target).is_ok() {
        return Ok(target);
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&open_dir)?;
    let mut input = std::fs::File::open(path)?;
    std::io::copy(&mut input, temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(&target) {
        Ok(_) => Ok(target),
        Err(error) => match std::fs::symlink_metadata(&target) {
            Ok(metadata)
                if metadata.file_type().is_file()
                    && !metadata.file_type().is_symlink()
                    && chatvault_metadata::verify_file_hash(&target, stem).is_ok() =>
            {
                Ok(target)
            }
            _ => Err(error.error),
        },
    }
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

/// 查询对象代表信息：名称、哈希、本机可读路径
struct ObjectExportInfo {
    original_name: String,
    hash: String,
    open_path: Option<String>,
}

fn load_object_export_info(
    db: &chatvault_index::Database,
    _device_id: &str,
    object_id: &str,
) -> std::result::Result<ObjectExportInfo, String> {
    let conn = db.connection();
    let (original_name, hash): (String, String) = conn
        .query_row(
            r#"
            SELECT
                COALESCE(
                    (SELECT r.original_name FROM file_records r
                     WHERE r.object_id = ?1
                       AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
                     ORDER BY r.file_time DESC, r.record_id LIMIT 1),
                    o.extension
                ),
                o.hash
            FROM file_objects o
            WHERE o.object_id = ?1
            "#,
            [object_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("未找到内容对象: {e}"))?;

    let open_path: Option<String> = conn
        .query_row(
            r#"
            SELECT CASE
                WHEN EXISTS(
                    SELECT 1 FROM local_files l
                    JOIN file_records r ON r.record_id = l.record_id
                    WHERE r.object_id = ?1
                      AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
                      AND l.original_path <> ''
                ) THEN (
                    SELECT l.original_path FROM local_files l
                    JOIN file_records r ON r.record_id = l.record_id
                    WHERE r.object_id = ?1
                      AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
                      AND l.original_path <> ''
                    ORDER BY r.file_time DESC LIMIT 1
                )
                ELSE (
                    SELECT l.cache_path FROM local_files l
                    JOIN file_records r ON r.record_id = l.record_id
                    WHERE r.object_id = ?1
                      AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
                      AND l.cache_path IS NOT NULL AND l.cache_path <> ''
                    ORDER BY r.file_time DESC LIMIT 1
                )
            END
            "#,
            [object_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .map_err(|e| e.to_string())?;

    // 磁盘不可读时视为无路径
    let open_path = open_path.filter(|p| {
        std::fs::symlink_metadata(p)
            .map(|m| m.file_type().is_file() && !m.file_type().is_symlink())
            .unwrap_or(false)
    });

    Ok(ObjectExportInfo {
        original_name,
        hash,
        open_path,
    })
}

fn safe_export_name(original_name: &str, hash: &str) -> String {
    if original_name.trim().is_empty() {
        format!("{}.bin", hash.chars().take(16).collect::<String>())
    } else {
        Path::new(original_name)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| original_name.to_string())
    }
}

/// 导出所需的连接与路径配置（避免在 await 间持有 Database）
struct ExportContext {
    vault_id: String,
    webdav_url: String,
    webdav_username: String,
    password: Option<String>,
    download_dir: PathBuf,
}

fn build_export_context(db: &chatvault_index::Database, vault_id: &str) -> ExportContext {
    let webdav_url = db
        .get_setting(setting_keys::WEBDAV_URL)
        .ok()
        .flatten()
        .unwrap_or_default();
    let webdav_username = db
        .get_setting(setting_keys::WEBDAV_USERNAME)
        .ok()
        .flatten()
        .unwrap_or_default();
    let download_dir_cfg = db
        .get_setting(setting_keys::DOWNLOAD_DIR)
        .ok()
        .flatten()
        .unwrap_or_default();
    let password = resolve_webdav_password(&webdav_url, &webdav_username, None);
    ExportContext {
        vault_id: vault_id.to_string(),
        webdav_url,
        webdav_username,
        password,
        download_dir: resolve_download_dir(&download_dir_cfg),
    }
}

async fn export_object_with_context(
    ctx: &ExportContext,
    info: &ObjectExportInfo,
) -> std::result::Result<DownloadResultDto, String> {
    std::fs::create_dir_all(&ctx.download_dir)
        .map_err(|e| format!("无法创建下载目录 {}: {e}", ctx.download_dir.display()))?;
    let safe_name = safe_export_name(&info.original_name, &info.hash);
    let dest = unique_dest_path(&ctx.download_dir, &safe_name);

    if let Some(src) = info.open_path.as_ref() {
        std::fs::copy(src, &dest).map_err(|e| format!("复制本地文件失败: {e}"))?;
        let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        let saved_name = dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| safe_name.clone());
        return Ok(DownloadResultDto {
            saved_path: dest.to_string_lossy().into_owned(),
            file_name: saved_name,
            size,
        });
    }

    if ctx.webdav_url.trim().is_empty() {
        return Err("无本地文件且尚未配置 WebDAV".into());
    }
    if ctx.password.is_none() {
        return Err("无本地文件且尚未保存 WebDAV 密码".into());
    }
    let client = WebDavClient::new(WebDavConfig {
        base_url: ctx.webdav_url.clone(),
        username: Some(ctx.webdav_username.clone()),
        password: ctx.password.clone(),
    })
    .map_err(|e| e.to_string())?;
    let remote_path = get_object_path(&ctx.vault_id, &info.hash);
    let size = client
        .download_to_path(&remote_path, &dest, &info.hash)
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

/// 批量导出内容对象到下载目录（含本地已可打开对象）
#[tauri::command]
pub async fn download_objects(
    object_ids: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<BatchResultDto, String> {
    let vault_id = state.vault_id().map_err(|e| e.to_string())?;
    let (ctx, infos) = {
        let db = state.get_db().map_err(|e| e.to_string())?;
        let ctx = build_export_context(&db, &vault_id);
        let mut infos = Vec::with_capacity(object_ids.len());
        for object_id in &object_ids {
            let info = load_object_export_info(&db, "", object_id).ok();
            infos.push((object_id.clone(), info));
        }
        (ctx, infos)
    };

    let mut items = Vec::with_capacity(object_ids.len());
    let mut ok_count = 0usize;
    let mut failed_count = 0usize;
    for (object_id, info) in infos {
        let original_name = info
            .as_ref()
            .map(|i| i.original_name.clone())
            .unwrap_or_default();
        let Some(info) = info else {
            failed_count += 1;
            items.push(BatchItemResultDto {
                object_id,
                original_name,
                status: "failed".into(),
                saved_path: None,
                released_bytes: None,
                error: Some("未找到内容对象".into()),
            });
            continue;
        };
        match export_object_with_context(&ctx, &info).await {
            Ok(result) => {
                ok_count += 1;
                items.push(BatchItemResultDto {
                    object_id,
                    original_name: result.file_name.clone(),
                    status: "ok".into(),
                    saved_path: Some(result.saved_path),
                    released_bytes: None,
                    error: None,
                });
            }
            Err(error) => {
                failed_count += 1;
                items.push(BatchItemResultDto {
                    object_id,
                    original_name,
                    status: "failed".into(),
                    saved_path: None,
                    released_bytes: None,
                    error: Some(error),
                });
            }
        }
    }
    Ok(BatchResultDto {
        total: object_ids.len(),
        ok_count,
        failed_count,
        released_bytes: 0,
        items,
    })
}

/// 释放选中对象的受控缓存副本
#[tauri::command]
pub async fn release_object_cache(
    object_ids: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<BatchResultDto, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let mut items = Vec::with_capacity(object_ids.len());
    let mut ok_count = 0usize;
    let mut failed_count = 0usize;
    let mut released_bytes = 0u64;
    for object_id in &object_ids {
        let original_name = {
            let conn = db.connection();
            conn.query_row(
                r#"
                SELECT COALESCE(
                    (SELECT r.original_name FROM file_records r
                     WHERE r.object_id = ?1
                       AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
                     ORDER BY r.file_time DESC LIMIT 1),
                    ''
                )
                "#,
                [object_id],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_default()
        };
        match db.release_object_cache(object_id) {
            Ok((true, bytes)) => {
                ok_count += 1;
                released_bytes = released_bytes.saturating_add(bytes);
                items.push(BatchItemResultDto {
                    object_id: object_id.clone(),
                    original_name,
                    status: "ok".into(),
                    saved_path: None,
                    released_bytes: Some(bytes),
                    error: None,
                });
            }
            Ok((false, _)) => {
                failed_count += 1;
                items.push(BatchItemResultDto {
                    object_id: object_id.clone(),
                    original_name,
                    status: "failed".into(),
                    saved_path: None,
                    released_bytes: None,
                    error: Some("尚未完成远端归档，无法释放缓存".into()),
                });
            }
            Err(e) => {
                failed_count += 1;
                items.push(BatchItemResultDto {
                    object_id: object_id.clone(),
                    original_name,
                    status: "failed".into(),
                    saved_path: None,
                    released_bytes: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    Ok(BatchResultDto {
        total: object_ids.len(),
        ok_count,
        failed_count,
        released_bytes,
        items,
    })
}

/// 删除选中对象的本机原文件与缓存映射
/// 后端强制：未完成归档/未发布元数据、路径不在采集源内时拒绝执行。
#[tauri::command]
pub async fn delete_object_local_files(
    object_ids: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<BatchResultDto, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let mut items = Vec::with_capacity(object_ids.len());
    let mut ok_count = 0usize;
    let mut failed_count = 0usize;
    let mut released_bytes = 0u64;
    for object_id in &object_ids {
        let original_name = {
            let conn = db.connection();
            conn.query_row(
                r#"
                SELECT COALESCE(
                    (SELECT r.original_name FROM file_records r
                     WHERE r.object_id = ?1
                       AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
                     ORDER BY r.file_time DESC LIMIT 1),
                    ''
                )
                "#,
                [object_id],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_default()
        };
        match db.delete_object_local_files(object_id) {
            Ok((_origins, _caches, bytes)) => {
                ok_count += 1;
                released_bytes = released_bytes.saturating_add(bytes);
                items.push(BatchItemResultDto {
                    object_id: object_id.clone(),
                    original_name,
                    status: "ok".into(),
                    saved_path: None,
                    released_bytes: Some(bytes),
                    error: None,
                });
            }
            Err(e) => {
                failed_count += 1;
                items.push(BatchItemResultDto {
                    object_id: object_id.clone(),
                    original_name,
                    status: "failed".into(),
                    saved_path: None,
                    released_bytes: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    Ok(BatchResultDto {
        total: object_ids.len(),
        ok_count,
        failed_count,
        released_bytes,
        items,
    })
}

/// 批量隐藏内容对象（软隐藏，可恢复；不中断备份）
#[tauri::command]
pub async fn library_hide_objects(
    object_ids: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<BatchResultDto, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    batch_visibility_op(
        &mut db,
        &device_id,
        &object_ids,
        |db, device_id, object_id| db.hide_object(device_id, object_id).map(|_| None),
    )
}

/// 批量恢复已隐藏对象
#[tauri::command]
pub async fn library_restore_objects(
    object_ids: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<BatchResultDto, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    batch_visibility_op(
        &mut db,
        &device_id,
        &object_ids,
        |db, device_id, object_id| db.restore_object(device_id, object_id).map(|_| None),
    )
}

/// 批量彻底删除已隐藏对象；成功后尝试删除 WebDAV 远端内容
#[tauri::command]
pub async fn library_purge_objects(
    object_ids: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<BatchResultDto, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    let vault_id = state.vault_id().map_err(|e| e.to_string())?;
    let webdav_url = db
        .get_setting(setting_keys::WEBDAV_URL)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let webdav_username = db
        .get_setting(setting_keys::WEBDAV_USERNAME)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let password = if webdav_url.trim().is_empty() {
        None
    } else {
        resolve_webdav_password(&webdav_url, &webdav_username, None)
    };
    let client = if webdav_url.trim().is_empty() || password.is_none() {
        None
    } else {
        Some(
            WebDavClient::new(WebDavConfig {
                base_url: webdav_url,
                username: Some(webdav_username),
                password,
            })
            .map_err(|e| e.to_string())?,
        )
    };

    let mut items = Vec::with_capacity(object_ids.len());
    let mut ok_count = 0usize;
    let mut failed_count = 0usize;
    for object_id in &object_ids {
        let original_name = load_representative_name(&db, object_id);
        match db.purge_object(&device_id, object_id) {
            Ok(outcome) => {
                let mut remote_error: Option<String> = None;
                if let Some(client) = client.as_ref() {
                    let remote_path = get_object_path(&vault_id, &outcome.hash);
                    match client.delete_resource(&remote_path).await {
                        Ok(()) => {
                            // 仅绑定 WebDAV 且远端删除成功时标记已清理，便于后续重试残留。
                            let _ = db.mark_remote_purged_cleaned(object_id);
                        }
                        Err(e) => {
                            remote_error = Some(format!("索引已删除，远端清理失败: {e}"));
                        }
                    }
                }
                if let Some(err) = remote_error {
                    failed_count += 1;
                    items.push(BatchItemResultDto {
                        object_id: object_id.clone(),
                        original_name,
                        status: "partial".into(),
                        saved_path: None,
                        released_bytes: Some(outcome.size),
                        error: Some(err),
                    });
                } else {
                    ok_count += 1;
                    items.push(BatchItemResultDto {
                        object_id: object_id.clone(),
                        original_name,
                        status: "ok".into(),
                        saved_path: None,
                        released_bytes: Some(outcome.size),
                        error: if client.is_none() {
                            Some("未绑定 WebDAV，仅删除本机库记录".into())
                        } else {
                            None
                        },
                    });
                }
            }
            Err(e) => {
                failed_count += 1;
                items.push(BatchItemResultDto {
                    object_id: object_id.clone(),
                    original_name,
                    status: "failed".into(),
                    saved_path: None,
                    released_bytes: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    Ok(BatchResultDto {
        total: object_ids.len(),
        ok_count,
        failed_count,
        released_bytes: 0,
        items,
    })
}

/// 查询仍待远端清理的 purge 对象数量
#[tauri::command]
pub async fn count_pending_remote_purges(
    state: State<'_, AppState>,
) -> std::result::Result<usize, String> {
    let db = state.get_db().map_err(|e| e.to_string())?;
    db.count_pending_remote_purges().map_err(|e| e.to_string())
}

/// 重试删除彻底删除后仍残留的 WebDAV 远端对象（DELETE 幂等，404 视为成功）
#[tauri::command]
pub async fn cleanup_purge_remote(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> std::result::Result<BatchResultDto, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    let vault_id = state.vault_id().map_err(|e| e.to_string())?;
    let webdav_url = db
        .get_setting(setting_keys::WEBDAV_URL)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let webdav_username = db
        .get_setting(setting_keys::WEBDAV_USERNAME)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let password = if webdav_url.trim().is_empty() {
        None
    } else {
        resolve_webdav_password(&webdav_url, &webdav_username, None)
    };
    if webdav_url.trim().is_empty() || password.is_none() {
        return Err("尚未配置 WebDAV，无法清理网盘残留".into());
    }
    let client = WebDavClient::new(WebDavConfig {
        base_url: webdav_url,
        username: Some(webdav_username),
        password,
    })
    .map_err(|e| e.to_string())?;

    let pending = db
        .list_pending_remote_purges(limit.unwrap_or(200).clamp(1, 1000))
        .map_err(|e| e.to_string())?;
    let mut items = Vec::with_capacity(pending.len());
    let mut ok_count = 0usize;
    let mut failed_count = 0usize;
    for entry in pending {
        let remote_path = get_object_path(&vault_id, &entry.hash);
        match client.delete_resource(&remote_path).await {
            Ok(()) => {
                db.mark_remote_purged_cleaned(&entry.object_id)
                    .map_err(|e| e.to_string())?;
                ok_count += 1;
                items.push(BatchItemResultDto {
                    object_id: entry.object_id.clone(),
                    original_name: String::new(),
                    status: "ok".into(),
                    saved_path: None,
                    released_bytes: Some(entry.size),
                    error: None,
                });
            }
            Err(e) => {
                failed_count += 1;
                items.push(BatchItemResultDto {
                    object_id: entry.object_id.clone(),
                    original_name: String::new(),
                    status: "failed".into(),
                    saved_path: None,
                    released_bytes: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    Ok(BatchResultDto {
        total: items.len(),
        ok_count,
        failed_count,
        released_bytes: 0,
        items,
    })
}

/// 读取对象代表文件名（用于批量结果展示）
fn load_representative_name(db: &chatvault_index::Database, object_id: &str) -> String {
    db.connection()
        .query_row(
            r#"
            SELECT COALESCE(
                (SELECT r.original_name FROM file_records r
                 WHERE r.object_id = ?1
                   AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
                 ORDER BY r.file_time DESC LIMIT 1),
                ''
            )
            "#,
            [object_id],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_default()
}

/// 统一批量可见性操作：逐对象执行并汇总结果。
fn batch_visibility_op(
    db: &mut chatvault_index::Database,
    device_id: &str,
    object_ids: &[String],
    mut op: impl FnMut(
        &mut chatvault_index::Database,
        &str,
        &str,
    ) -> std::result::Result<Option<u64>, chatvault_core::error::ChatVaultError>,
) -> std::result::Result<BatchResultDto, String> {
    let mut items = Vec::with_capacity(object_ids.len());
    let mut ok_count = 0usize;
    let mut failed_count = 0usize;
    for object_id in object_ids {
        let original_name = load_representative_name(db, object_id);
        match op(db, device_id, object_id) {
            Ok(bytes) => {
                ok_count += 1;
                items.push(BatchItemResultDto {
                    object_id: object_id.clone(),
                    original_name,
                    status: "ok".into(),
                    saved_path: None,
                    released_bytes: bytes,
                    error: None,
                });
            }
            Err(e) => {
                failed_count += 1;
                items.push(BatchItemResultDto {
                    object_id: object_id.clone(),
                    original_name,
                    status: "failed".into(),
                    saved_path: None,
                    released_bytes: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    Ok(BatchResultDto {
        total: object_ids.len(),
        ok_count,
        failed_count,
        released_bytes: 0,
        items,
    })
}

/// 立即回收受控缓存；force_all 为 true 时忽略保留天数与容量目标
#[tauri::command]
pub async fn reclaim_cache_now(
    force_all: Option<bool>,
    state: State<'_, AppState>,
) -> std::result::Result<u64, String> {
    let mut db = state.get_db().map_err(|e| e.to_string())?;
    if force_all.unwrap_or(false) {
        db.reclaim_cache_force().map_err(|e| e.to_string())
    } else {
        db.reclaim_cache().map_err(|e| e.to_string())
    }
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

    #[test]
    fn extension_open_copy_keeps_plaintext_and_real_suffix() {
        let tmp =
            std::env::temp_dir().join(format!("chatvault-open-copy-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let bytes = b"verified image bytes";
        let hash = chatvault_metadata::compute_blake3_bytes(bytes).hex_hash;
        let source = tmp.join(&hash);
        std::fs::write(&source, bytes).unwrap();
        let opened = prepare_extension_open_path(&source, "png").unwrap();
        assert_eq!(opened.extension().and_then(|v| v.to_str()), Some("png"));
        assert_eq!(std::fs::read(opened).unwrap(), bytes);
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        let _ = std::fs::remove_dir_all(tmp);
    }
}
