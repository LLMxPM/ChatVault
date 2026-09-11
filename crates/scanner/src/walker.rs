//! # 目录遍历模块
//!
//! 提供基于 walkdir 的受控目录遍历引擎，自动过滤临时文件、隐藏系统目录，
//! 并产出候选文件列表供上层做稳定性检测与入库。

use chatvault_core::error::{ChatVaultError, Result};
use std::path::{Path, PathBuf};
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

/// 扫描指定目录并收集符合条件的常规文件路径
///
/// 职责: 递归遍历指定目录，过滤常见临时垃圾文件、隐藏文件与空文件
/// 输入:
///   - `root`: 根目录路径
///   - `options`: 过滤选项
/// 输出: 路径列表 `Result<Vec<PathBuf>>`
pub fn scan_directory<P: AsRef<Path>>(root: P, options: &ScanOptions) -> Result<Vec<PathBuf>> {
    let r = root.as_ref();
    if !r.exists() {
        return Err(ChatVaultError::FileNotFound {
            path: r.display().to_string(),
        });
    }

    let mut walker = WalkDir::new(r).follow_links(false);
    if let Some(depth) = options.max_depth {
        walker = walker.max_depth(depth);
    }

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
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_walker() {
        let temp_dir = std::env::temp_dir().join("chatvault_walker_test");
        let _ = std::fs::create_dir_all(&temp_dir);

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

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
