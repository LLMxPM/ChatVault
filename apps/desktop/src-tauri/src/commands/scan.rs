// ChatVault 桌面命令：scan 职责实现与前端错误映射。
use super::*;

/// 探测本机微信 4.x 账号列表与附件目录
///
/// # 输出
/// - `Result<Vec<WechatAccountDto>, String>`: 微信 4.x 账号及统计概览
#[tauri::command]
pub async fn detect_wechat_accounts() -> std::result::Result<Vec<WechatAccountDto>, String> {
    let root = match WeChat4Detector::detect_root() {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };

    let accounts = WeChat4Detector::find_accounts(&root).unwrap_or_default();
    let mut dtos = Vec::new();

    for acc in accounts {
        let files = WeChat4Parser::parse_account_files(&acc).unwrap_or_default();
        dtos.push(WechatAccountDto {
            account_id: acc.account_id,
            source_dir: acc.files_dir.to_string_lossy().to_string(),
            files_count_estimated: files.len(),
        });
    }

    Ok(dtos)
}

/// 触发针对指定微信账号与通用目录的增量去重扫描
///
/// # 输入
/// - `request`: 包含需要扫描的账号列表与通用文件夹列表
/// - `state`: 应用全局上下文
#[tauri::command]
pub async fn run_scan(
    request: ScanRequestDto,
    state: State<'_, AppState>,
) -> std::result::Result<ScanResultDto, String> {
    let start_time = Instant::now();
    let mut db = state.get_db().map_err(|e| e.to_string())?;

    let mut all_discovered = Vec::new();

    // 1. 扫描微信账号
    if let Ok(root) = WeChat4Detector::detect_root() {
        let detected_accounts = WeChat4Detector::find_accounts(&root).unwrap_or_default();
        for acc in detected_accounts {
            if request.target_accounts.contains(&acc.account_id) {
                if let Ok(files) = WeChat4Parser::parse_account_files(&acc) {
                    all_discovered.extend(files);
                }
            }
        }
    }

    // 2. 扫描通用自定义文件夹
    for folder in &request.custom_folders {
        let p = PathBuf::from(folder);
        if p.exists() {
            if let Ok(files) = GenericFolderParser::parse(&p) {
                all_discovered.extend(files);
            }
        }
    }

    let total_discovered = all_discovered.len();
    let mut total_new_objects = 0;
    let mut total_skipped = 0;

    // 3. 增量原子入库与去重
    let device_id = state.device_id().map_err(|e| e.to_string())?;
    for file in all_discovered {
        match db.ingest_file(&file, &device_id) {
            Ok(IngestResult::Indexed { is_new_object, .. }) => {
                if is_new_object {
                    total_new_objects += 1;
                } else {
                    total_skipped += 1;
                }
            }
            Ok(IngestResult::Skipped { .. }) => {
                total_skipped += 1;
            }
            Err(e) => {
                eprintln!("入库失败 {}: {}", file.absolute_path, e);
            }
        }
    }

    Ok(ScanResultDto {
        total_discovered,
        total_new_objects,
        total_skipped,
        duration_ms: start_time.elapsed().as_millis(),
    })
}
