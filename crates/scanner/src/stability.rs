//! # 文件稳定性检测模块
//!
//! 用于判断微信或外部程序下载的文件是否已经写入完成并处于静止状态。
//! 防止在文件还在传输、写入或被独占锁定时进行哈希计算和归档，从而避免损坏归档数据。

use chatvault_core::error::{ChatVaultError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// 稳定性检测快照
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSnapshot {
    /// 文件大小（字节数）
    pub size: u64,
    /// 最后修改时间
    pub modified: SystemTime,
}

/// 获取当前文件的状态快照
///
/// 职责: 读取指定路径文件的长度与修改时间，并验证是否可读
/// 输入: `path`: 目标文件路径
/// 输出: `Result<FileSnapshot>`
pub fn get_snapshot<P: AsRef<Path>>(path: P) -> Result<FileSnapshot> {
    let p = path.as_ref();
    let metadata = std::fs::metadata(p).map_err(|e| ChatVaultError::FileNotFound {
        path: format!("{}: {}", p.display(), e),
    })?;

    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let size = metadata.len();

    // 尝试以只读方式打开以检测是否被独占写入锁定
    let mut file = File::open(p)?;
    if size > 0 {
        let mut buf = [0u8; 1];
        let _ = file.read(&mut buf);
    }

    Ok(FileSnapshot { size, modified })
}

/// 异步检测文件是否处于静止稳定状态
///
/// 职责: 比对当前快照与经过 delay 延时后的状态，如果文件大小和修改时间保持一致，判定为稳定
/// 输入:
///   - `path`: 文件路径
///   - `check_interval`: 两次比对之间的等待时长（如 200ms ~ 1000ms）
/// 输出: `Result<bool>`: true 表示稳定可用，false 表示仍在变化中
/// 关键约束:
///   - 必须能正常读取，否则返回 Err 或 false
pub async fn check_file_stability_async<P: AsRef<Path>>(
    path: P,
    check_interval: Duration,
) -> Result<bool> {
    let p = path.as_ref();
    let s1 = match get_snapshot(p) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };

    tokio::time::sleep(check_interval).await;

    let s2 = match get_snapshot(p) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };

    Ok(s1 == s2)
}

/// 同步检测文件是否处于静止稳定状态
///
/// 职责: 阻塞式比对两次文件快照
/// 输入:
///   - `path`: 文件路径
///   - `check_interval`: 等待时间
/// 输出: `Result<bool>`
pub fn check_file_stability_sync<P: AsRef<Path>>(
    path: P,
    check_interval: Duration,
) -> Result<bool> {
    let p = path.as_ref();
    let s1 = match get_snapshot(p) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };

    std::thread::sleep(check_interval);

    let s2 = match get_snapshot(p) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };

    Ok(s1 == s2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_sync_stability() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("chatvault_stability_test.txt");
        let mut f = File::create(&test_file).unwrap();
        f.write_all(b"stable file test").unwrap();
        drop(f);

        let stable = check_file_stability_sync(&test_file, Duration::from_millis(50)).unwrap();
        assert!(stable);

        let _ = std::fs::remove_file(test_file);
    }
}
