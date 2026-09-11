//! # 通用目录适配器
//!
//! 允许用户指定任意本地文件夹，递归提取该目录下的所有合法文件，
//! 生成标准 `DiscoveredFile`，提供统一的扫描归档接口。

use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::DiscoveredFile;
use chatvault_scanner::walker::{scan_directory, ScanOptions};
use chrono::{DateTime, Utc};
use std::path::Path;

/// 通用目录解析器
pub struct GenericFolderParser;

impl GenericFolderParser {
    /// 扫描指定的本地文件夹
    ///
    /// 职责: 递归遍历用户指定的任意本地文件夹，生成候选待处理文件清单
    /// 输入:
    ///   - `folder`: 目标目录路径
    /// 输出: `Result<Vec<DiscoveredFile>>`
    pub fn parse<P: AsRef<Path>>(folder: P) -> Result<Vec<DiscoveredFile>> {
        let f = folder.as_ref();
        if !f.exists() {
            return Err(ChatVaultError::FileNotFound {
                path: f.display().to_string(),
            });
        }

        let options = ScanOptions {
            max_depth: None,
            skip_hidden: true,
            min_size: 1,
        };

        let paths = scan_directory(f, &options)?;
        let mut discovered = Vec::with_capacity(paths.len());

        for path in paths {
            let metadata = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("无法读取文件元数据 {}: {}", path.display(), e);
                    continue;
                }
            };

            let file_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            let modified_system = metadata.modified().map_err(|e| ChatVaultError::Io(e))?;
            let modified_time: DateTime<Utc> = modified_system.into();

            let parent_name = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());

            discovered.push(DiscoveredFile {
                source_type: "generic-folder".to_string(),
                account_id: None,
                absolute_path: path.to_string_lossy().to_string(),
                file_name,
                file_size: metadata.len(),
                modified_time,
                conversation_hint: parent_name,
            });
        }

        Ok(discovered)
    }
}
