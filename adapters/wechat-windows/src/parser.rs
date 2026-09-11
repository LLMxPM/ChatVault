//! # 微信 4.x 附件解析与提取模块
//!
//! 负责递归遍历微信 4.x 账号下的 `msg/file` 目录，提取文件大小、修改时间、
//! 原文件名与所属账号，打包为标准的 `DiscoveredFile` 实例供后续入库与归档。

use crate::detector::WeChatAccount;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::DiscoveredFile;
use chatvault_scanner::walker::{scan_directory, ScanOptions};
use chrono::{DateTime, Utc};
use std::path::Path;

/// 微信 4.x 文件解析器
pub struct WeChat4Parser;

impl WeChat4Parser {
    /// 扫描指定微信账号的文件列表
    ///
    /// 职责: 扫描 `account.files_dir`（即 `msg/file/`）下的所有文件，提取元数据
    /// 输入:
    ///   - `account`: 微信账号结构体
    /// 输出: `Result<Vec<DiscoveredFile>>`
    pub fn parse_account_files(account: &WeChatAccount) -> Result<Vec<DiscoveredFile>> {
        if !account.files_dir.exists() {
            tracing::info!(
                "账号 {} 的附件目录尚未生成: {}",
                account.account_id,
                account.files_dir.display()
            );
            return Ok(Vec::new());
        }

        Self::parse_folder(&account.files_dir, Some(&account.account_id))
    }

    /// 从任意给定的微信 4.x 目录（例如某个月份目录或自定义目录）提取文件
    ///
    /// 职责: 遍历目录提取 DiscoveredFile 并附带上下文标签
    /// 输入:
    ///   - `folder`: 目标目录路径
    ///   - `account_id`: 可选的账号标识
    /// 输出: `Result<Vec<DiscoveredFile>>`
    pub fn parse_folder<P: AsRef<Path>>(
        folder: P,
        account_id: Option<&str>,
    ) -> Result<Vec<DiscoveredFile>> {
        let f = folder.as_ref();
        let options = ScanOptions {
            max_depth: None,
            skip_hidden: true,
            min_size: 1, // 微信可能有占位 0 字节文件，跳过空文件
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

            // 月份目录不能证明会话身份；未验证来源保持未知。
            discovered.push(DiscoveredFile {
                source_type: "wechat-windows-4".to_string(),
                account_id: account_id.map(|s| s.to_string()),
                absolute_path: path.to_string_lossy().to_string(),
                file_name,
                file_size: metadata.len(),
                modified_time,
                conversation_hint: None,
            });
        }

        Ok(discovered)
    }
}
