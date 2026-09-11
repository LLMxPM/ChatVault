// ChatVault 本地受控暂存：复制稳定内容并计算哈希，避免源文件变化导致历史版本丢失。
use crate::hasher::{compute_blake3_file, HashResult};
use chatvault_core::error::{ChatVaultError, Result};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// 复制并验证稳定文件；返回不可变副本、实际内容哈希、实际修改时间。
pub fn stage_file(
    source: &Path,
    directory: &Path,
) -> Result<(PathBuf, HashResult, std::time::SystemTime)> {
    fs::create_dir_all(directory)?;
    let mut input = fs::File::open(source)?;
    let before = input.metadata()?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 65536];
    let mut size = 0;
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        temporary.write_all(&buffer[..n])?;
        hasher.update(&buffer[..n]);
        size += n as u64;
    }
    let after = fs::metadata(source)?;
    if before.len() != size
        || before.len() != after.len()
        || before.modified()? != after.modified()?
    {
        return Err(ChatVaultError::Scan(
            "复制期间文件发生变化，请重新扫描".into(),
        ));
    }
    // 独立复算源文件可发现同大小覆盖，副本写盘后才允许索引引用。
    let hash = hasher.finalize().to_hex().to_string();
    if compute_blake3_file(source)?.hex_hash != hash {
        return Err(ChatVaultError::Scan(
            "复制期间文件内容发生变化，请重新扫描".into(),
        ));
    }
    temporary.as_file().sync_all()?;
    let target = directory.join(&hash);
    if target.exists() {
        crate::verify_file_hash(&target, &hash)?;
    } else if let Err(e) = temporary.persist_noclobber(&target) {
        if target.exists() {
            crate::verify_file_hash(&target, &hash)?;
        } else {
            return Err(e.error.into());
        }
    }
    Ok((
        target,
        HashResult {
            object_id: format!("blake3:{hash}"),
            hex_hash: hash,
            bytes_read: size,
        },
        before.modified()?,
    ))
}
