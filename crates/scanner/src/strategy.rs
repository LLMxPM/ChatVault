//! 增量扫描策略：根据来源目录结构决定是否跳过目录。

use std::time::SystemTime;
use walkdir::DirEntry;

/// 控制扫描器是否继续进入某个目录。
///
/// 策略只负责目录发现，文件是否已经入库由上层通过本地文件状态复核决定。
pub trait IncrementalScanStrategy {
    /// 返回是否应该继续遍历当前条目。
    fn should_descend(&self, entry: &DirEntry, since: Option<SystemTime>) -> bool;
}

/// 完整遍历策略，不根据目录时间裁剪路径。
///
/// 适用于目录结构不稳定、无法从父目录 mtime 可靠判断新增文件的来源。
#[derive(Debug, Clone, Copy, Default)]
pub struct FullScanStrategy;

impl IncrementalScanStrategy for FullScanStrategy {
    fn should_descend(&self, _entry: &DirEntry, _since: Option<SystemTime>) -> bool {
        true
    }
}

/// 在指定目录层级使用目录 mtime 进行裁剪的策略。
///
/// 例如扫描根目录为 `msg/file` 时，`prune_depth = 1` 表示只检查其直接子目录
/// （通常是 `YYYY-MM`）。该目录没有变化时跳过其全部内容；更深层目录不会单独
/// 裁剪，因此要求来源的固定布局能保证新增文件会更新这一层目录的 mtime。
#[derive(Debug, Clone, Copy)]
pub struct MtimeAtDepthStrategy {
    prune_depth: usize,
}

impl MtimeAtDepthStrategy {
    /// 创建目录 mtime 裁剪策略。
    pub const fn new(prune_depth: usize) -> Self {
        Self { prune_depth }
    }
}

impl IncrementalScanStrategy for MtimeAtDepthStrategy {
    fn should_descend(&self, entry: &DirEntry, since: Option<SystemTime>) -> bool {
        let Some(since) = since else {
            return true;
        };

        if !entry.file_type().is_dir() || entry.depth() == 0 || entry.depth() != self.prune_depth {
            return true;
        }

        match entry.metadata() {
            Ok(metadata) => metadata
                .modified()
                .map(|modified| modified > since)
                .unwrap_or(true),
            Err(_) => true,
        }
    }
}
