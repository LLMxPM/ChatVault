//! # 微信 4.x 附件解析与提取模块
//!
//! 负责递归遍历微信 4.x 账号下的 `msg/file` 目录，提取文件大小、修改时间、
//! 原文件名与所属账号，打包为标准的 `DiscoveredFile` 实例供后续入库与归档。

use crate::detector::WeChatAccount;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::DiscoveredFile;
use chatvault_scanner::strategy::MtimeAtDepthStrategy;
use chatvault_scanner::walker::{scan_directory_with_strategy, ScanOptions};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::time::SystemTime;

/// 微信 4.x 文件解析器
pub struct WeChat4Parser;

impl WeChat4Parser {
    /// 全量扫描指定微信账号的附件目录
    pub fn parse_account_files(account: &WeChatAccount) -> Result<Vec<DiscoveredFile>> {
        Self::parse_account_files_since(account, None)
    }

    /// 按微信 4.x 固定目录结构增量发现附件；None 为全量
    ///
    /// 职责: 扫描 `account.files_dir`（即 `msg/file/`）下的文件，提取元数据
    /// 输入:
    ///   - `account`: 微信账号结构体
    ///   - `since`: 增量起点；月份目录 mtime 不晚于该时刻时跳过对应子树
    /// 输出: `Result<Vec<DiscoveredFile>>`
    pub fn parse_account_files_since(
        account: &WeChatAccount,
        since: Option<SystemTime>,
    ) -> Result<Vec<DiscoveredFile>> {
        if !account.files_dir.exists() {
            tracing::info!(
                "账号 {} 的附件目录尚未生成: {}",
                account.account_id,
                account.files_dir.display()
            );
            return Ok(Vec::new());
        }

        Self::parse_folder_since(&account.files_dir, Some(&account.account_id), since)
    }

    /// 从任意给定的微信 4.x 目录提取文件（全量）
    pub fn parse_folder<P: AsRef<Path>>(
        folder: P,
        account_id: Option<&str>,
    ) -> Result<Vec<DiscoveredFile>> {
        Self::parse_folder_since(folder, account_id, None)
    }

    /// 按微信 4.x 固定目录策略增量提取文件并附带上下文标签
    pub fn parse_folder_since<P: AsRef<Path>>(
        folder: P,
        account_id: Option<&str>,
        since: Option<SystemTime>,
    ) -> Result<Vec<DiscoveredFile>> {
        let f = folder.as_ref();
        let options = ScanOptions {
            max_depth: None,
            skip_hidden: true,
            min_size: 1, // 微信可能有占位 0 字节文件，跳过空文件
        };

        // 微信 4.x 的 files 根目录下固定按月份分目录，深度 1 是安全的裁剪边界。
        let strategy = MtimeAtDepthStrategy::new(1);
        let paths = scan_directory_with_strategy(f, &options, since, &strategy)?;
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
