//! # 目录遍历模块
//!
//! 提供基于 walkdir 的受控目录遍历引擎，自动过滤临时文件、隐藏系统目录，
//! 并产出候选文件列表供上层做稳定性检测与入库。

use crate::strategy::{FullScanStrategy, IncrementalScanStrategy};
use chatvault_core::error::{ChatVaultError, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

/// 遍历过滤选项
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// 最大遍历深度（None 表示不限制）
    pub max_depth: Option<usize>,
    /// 是否跳过以点开头的隐藏文件或目录
    pub skip_hidden: bool,
    /// 最小文件字节数（过滤 0 字节空文件）
    pub min_size: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            skip_hidden: true,
            min_size: 1,
        }
    }
}

/// 使用完整遍历策略扫描指定目录并收集符合条件的常规文件路径。
///
/// 职责: 递归遍历指定目录，过滤常见临时垃圾文件、隐藏文件与空文件
/// 输入:
///   - `root`: 根目录路径
///   - `options`: 过滤选项
///     输出: 路径列表 `Result<Vec<PathBuf>>`
pub fn scan_directory<P: AsRef<Path>>(root: P, options: &ScanOptions) -> Result<Vec<PathBuf>> {
    scan_directory_with_strategy(root, options, None, &FullScanStrategy)
}

/// 按指定增量策略扫描目录，并收集符合条件的常规文件路径。
///
/// 职责: 执行通用目录遍历与文件过滤，将“是否裁剪目录”的决定委托给来源策略
/// 输入:
///   - `root`: 根目录路径
///   - `options`: 文件过滤选项
///   - `since`: 增量扫描起点；None 表示全量发现
///   - `strategy`: 当前来源使用的目录遍历策略
///     输出: 路径列表 `Result<Vec<PathBuf>>`
pub fn scan_directory_with_strategy<S: IncrementalScanStrategy + ?Sized, P: AsRef<Path>>(
    root: P,
    options: &ScanOptions,
    since: Option<SystemTime>,
    strategy: &S,
) -> Result<Vec<PathBuf>> {
    let r = root.as_ref();
    if !r.exists() {
        return Err(ChatVaultError::FileNotFound {
            path: r.display().to_string(),
        });
    }

    let mut builder = WalkDir::new(r).follow_links(false);
    if let Some(depth) = options.max_depth {
        builder = builder.max_depth(depth);
    }

    let walker = builder
        .into_iter()
        .filter_entry(|entry| strategy.should_descend(entry, since));

    let mut files = Vec::new();

    for entry_res in walker {
        let entry = match entry_res {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("扫描目录跳过不可访问项: {}", e);
                continue;
            }
        };

        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // 过滤临时文件与隐藏文件
        if options.skip_hidden && file_name.starts_with('.') {
            continue;
        }
        if file_name.starts_with("~$")
            || file_name.ends_with(".tmp")
            || file_name.ends_with(".crdownload")
            || file_name.ends_with(".part")
        {
            continue;
        }

        // 检查大小约束
        if let Ok(metadata) = entry.metadata() {
            if metadata.len() < options.min_size {
                continue;
            }
        }

        files.push(path.to_path_buf());
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::MtimeAtDepthStrategy;
    use std::fs::File;
    use std::io::Write;
    use std::time::Duration;

    #[test]
    fn test_walker() {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("系统时间应晚于 Unix epoch")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("chatvault_walker_test_{unique}"));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let valid_file = temp_dir.join("test_doc.pdf");
        let tmp_file = temp_dir.join("~$temp.docx");
        let empty_file = temp_dir.join("empty.txt");

        let mut f1 = File::create(&valid_file).unwrap();
        f1.write_all(b"content").unwrap();
        drop(f1);

        let mut f2 = File::create(&tmp_file).unwrap();
        f2.write_all(b"tmp").unwrap();
        drop(f2);

        let f3 = File::create(&empty_file).unwrap();
        drop(f3);

        let options = ScanOptions::default();
        let res = scan_directory(&temp_dir, &options).unwrap();

        assert_eq!(res.len(), 1);
        assert_eq!(res[0], valid_file);

        // 固定层级策略：根下文件仍在；深度 1 子树因 mtime 不晚于 since 被剪掉。
        let sub = temp_dir.join("subdir");
        std::fs::create_dir_all(&sub).unwrap();
        let mut f4 = File::create(sub.join("nested.pdf")).unwrap();
        f4.write_all(b"nested").unwrap();
        drop(f4);

        let res_full = scan_directory(&temp_dir, &ScanOptions::default()).unwrap();
        assert_eq!(res_full.len(), 2);

        let res_inc = scan_directory_with_strategy(
            &temp_dir,
            &ScanOptions::default(),
            Some(SystemTime::now() + Duration::from_secs(60)),
            &MtimeAtDepthStrategy::new(1),
        )
        .unwrap();
        assert_eq!(res_inc.len(), 1);
        assert_eq!(res_inc[0], valid_file);

        // 通用目录策略不依赖父目录 mtime，增量时仍能发现深层新增文件。
        let res_full_walk = scan_directory_with_strategy(
            &temp_dir,
            &ScanOptions::default(),
            Some(SystemTime::now() + Duration::from_secs(60)),
            &FullScanStrategy,
        )
        .unwrap();
        assert_eq!(res_full_walk.len(), 2);

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
