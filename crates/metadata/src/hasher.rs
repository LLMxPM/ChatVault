//! # BLAKE3 哈希计算模块
//!
//! 提供基于 BLAKE3 算法的文件流式哈希计算、对象 ID 生成与哈希一致性校验，
//! 采用 64KB 缓冲区进行流式分块读取，支持超大文件的高性能哈希计算，避免内存耗尽。

use chatvault_core::error::{ChatVaultError, Result};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// 哈希计算结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashResult {
    /// 完整十六进制哈希字符串
    pub hex_hash: String,
    /// 包含算法前缀的标准对象 ID，如 `blake3:abc123...`
    pub object_id: String,
    /// 文件实际读取的字节大小
    pub bytes_read: u64,
}

/// 计算指定路径文件的 BLAKE3 哈希值
///
/// 职责: 打开文件，流式分块读取并使用 BLAKE3 计算完整哈希
/// 输入:
///   - `path`: 待计算的目标文件路径
///     输出:
///   - `Result<HashResult>`: 计算成功的哈希与对象ID，或 IO 异常
///     关键约束:
///   - 文件必须存在且具备可读权限
///   - 采用 64KB 缓冲流读取，对 GB 级大文件友好
pub fn compute_blake3_file<P: AsRef<Path>>(path: P) -> Result<HashResult> {
    let p = path.as_ref();
    let file = File::open(p).map_err(|e| ChatVaultError::FileNotFound {
        path: format!("{}: {}", p.display(), e),
    })?;

    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total_bytes = 0u64;

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        total_bytes += n as u64;
    }

    let hash_output = hasher.finalize();
    let hex_hash = hash_output.to_hex().to_string();
    let object_id = format!("blake3:{}", hex_hash);

    Ok(HashResult {
        hex_hash,
        object_id,
        bytes_read: total_bytes,
    })
}

/// 计算内存字节切片的 BLAKE3 哈希
///
/// 职责: 直接对内存中的 byte buffer 计算哈希
/// 输入: `bytes`: 字节切片
/// 输出: `HashResult`
pub fn compute_blake3_bytes(bytes: &[u8]) -> HashResult {
    let hash_output = blake3::hash(bytes);
    let hex_hash = hash_output.to_hex().to_string();
    let object_id = format!("blake3:{}", hex_hash);

    HashResult {
        hex_hash,
        object_id,
        bytes_read: bytes.len() as u64,
    }
}

/// 验证给定的哈希与实际计算结果是否一致
///
/// 职责: 校验文件内容是否与预期哈希匹配
/// 输入:
///   - `path`: 本地文件路径
///   - `expected_hex`: 期望的十六进制哈希（可含也可不含 `blake3:` 前缀）
///     输出: 一致返回 Ok(())，否则返回 `ChatVaultError::HashMismatch`
pub fn verify_file_hash<P: AsRef<Path>>(path: P, expected_hex: &str) -> Result<()> {
    let clean_expected = expected_hex.trim_start_matches("blake3:");
    let res = compute_blake3_file(path)?;
    if res.hex_hash.eq_ignore_ascii_case(clean_expected) {
        Ok(())
    } else {
        Err(ChatVaultError::HashMismatch {
            expected: clean_expected.to_string(),
            actual: res.hex_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_compute_bytes() {
        let data = b"ChatVault test data";
        let res = compute_blake3_bytes(data);
        assert!(!res.hex_hash.is_empty());
        assert!(res.object_id.starts_with("blake3:"));
        assert_eq!(res.bytes_read, data.len() as u64);
    }

    #[test]
    fn test_compute_file() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("chatvault_test_hasher.txt");
        let mut f = File::create(&test_file).unwrap();
        f.write_all(b"Hello ChatVault Hasher").unwrap();
        drop(f);

        let res = compute_blake3_file(&test_file).unwrap();
        assert!(verify_file_hash(&test_file, &res.hex_hash).is_ok());

        let _ = std::fs::remove_file(test_file);
    }
}
