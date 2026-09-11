//! # WebDAV 远端存储格式与路径规划
//!
//! 实现 ChatVault V1 存储规范下的远端路径格式化生成器与解析器。
//! 包含内容寻址对象路径、Vault 配置路径以及上传暂存路径的标准化管理。

/// 生成内容对象的 WebDAV 存储相对路径
///
/// 职责: 将哈希值转换为二级分片目录路径，避免单目录下文件过多引起 WebDAV 服务性能下降
/// 输入:
///   - `vault_id`: 资料库标识
///   - `hex_hash`: 十六进制哈希字符串（至少包含 4 个字符）
/// 输出: 相对路径，例如 `ChatVault/<vault_id>/objects/blake3/ab/cd/abcdef123456...`
pub fn get_object_path(vault_id: &str, hex_hash: &str) -> String {
    let clean_hash = hex_hash.trim_start_matches("blake3:");
    let p1 = if clean_hash.len() >= 2 {
        &clean_hash[0..2]
    } else {
        "00"
    };
    let p2 = if clean_hash.len() >= 4 {
        &clean_hash[2..4]
    } else {
        "00"
    };
    format!(
        "ChatVault/{}/objects/blake3/{}/{}/{}",
        vault_id, p1, p2, clean_hash
    )
}

/// 获取资料库配置文件的 WebDAV 相对路径
///
/// 职责: 返回 Vault 配置文件的标准存储路径
/// 输入: `vault_id`: 资料库标识
/// 输出: 例如 `ChatVault/<vault_id>/config/vault.json`
pub fn get_config_path(vault_id: &str) -> String {
    format!("ChatVault/{}/config/vault.json", vault_id)
}

/// 获取上传暂存文件的 WebDAV 相对路径
///
/// 职责: 返回文件在归档上传阶段的临时暂存路径，待校验无误后再通过 MOVE 变为正式对象
/// 输入:
///   - `vault_id`: 资料库标识
///   - `device_id`: 设备标识
///   - `upload_id`: 本次上传任务唯一标识
/// 输出: 例如 `ChatVault/<vault_id>/staging/<device_id>/<upload_id>`
pub fn get_staging_path(vault_id: &str, device_id: &str, upload_id: &str) -> String {
    format!(
        "ChatVault/{}/staging/{}/{}",
        vault_id, device_id, upload_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths() {
        let vault = "test-vault";
        let hash = "a1b2c3d4e5f6";
        assert_eq!(
            get_object_path(vault, hash),
            "ChatVault/test-vault/objects/blake3/a1/b2/a1b2c3d4e5f6"
        );
        assert_eq!(
            get_config_path(vault),
            "ChatVault/test-vault/config/vault.json"
        );
        assert_eq!(
            get_staging_path(vault, "dev1", "task99"),
            "ChatVault/test-vault/staging/dev1/task99"
        );
    }
}
