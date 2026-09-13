//! # 微信统计参数白名单扫描与密钥派生
//!
//! 只枚举 `%APPDATA%/Tencent/xwechat/{net,ilink}/kvcomm` 下符合
//! `key_<code>_....statistic` 命名的文件名，不读取 MMKV 正文。
//! 派生出的 AES/XOR 参数仅在任务内存中存在。

use chatvault_core::error::{ChatVaultError, Result};
use md5::{Digest, Md5};
use std::path::{Path, PathBuf};

/// 参数扫描失败原因（不携带密钥值）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterScanError {
    /// 白名单目录不存在或不可读
    DirectoryUnavailable,
    /// 未找到任何 `key_<code>_*.statistic` 文件名
    NoCodesFound,
    /// 候选数量超过上限
    CandidateLimitExceeded,
}

/// 单个账号的密钥材料（仅任务内存，禁止持久化或日志）
#[derive(Clone)]
pub struct AccountKeyMaterial {
    /// 规范化后的账号标识（用于 AES key 派生）
    pub account_id: String,
    /// 从统计文件名提取的数字 code
    pub code: u32,
    /// AES-128 密钥的 ASCII 字节（16 字节）
    pub aes_key: [u8; 16],
    /// XOR 还原字节
    pub xor_byte: u8,
}

impl std::fmt::Debug for AccountKeyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountKeyMaterial")
            .field("account_id", &self.account_id)
            .field("code", &"[redacted]")
            .field("aes_key", &"[redacted]")
            .field("xor_byte", &"[redacted]")
            .finish()
    }
}

/// 默认白名单目录
pub fn default_kvcomm_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(appdata) = dirs::data_dir() {
        // Windows 上 dirs::data_dir() 返回 %APPDATA%
        let base = appdata.join("Tencent").join("xwechat");
        dirs.push(base.join("net").join("kvcomm"));
        dirs.push(base.join("ilink").join("kvcomm"));
    }
    dirs
}

/// 扫描白名单目录，提取去重后的 code 集合。
///
/// 只读取文件名，不打开文件内容。code 来自 `key_<digits>_*.statistic` 中的数字段。
pub fn scan_parameter_codes(dirs: &[PathBuf]) -> std::result::Result<Vec<u32>, ParameterScanError> {
    let mut codes = std::collections::BTreeSet::new();
    let mut any_dir = false;

    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        any_dir = true;
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = match name.to_str() {
                Some(n) => n,
                None => continue,
            };
            if let Some(code) = parse_code_from_statistic_name(name) {
                codes.insert(code);
            }
        }
    }

    if !any_dir {
        return Err(ParameterScanError::DirectoryUnavailable);
    }
    if codes.is_empty() {
        return Err(ParameterScanError::NoCodesFound);
    }
    Ok(codes.into_iter().collect())
}

/// 从统计文件名解析 code：`key_<digits>_*.statistic` 形态
fn parse_code_from_statistic_name(name: &str) -> Option<u32> {
    if !name.ends_with(".statistic") {
        return None;
    }
    let rest = name.strip_prefix("key_")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    // 退出残留 code=0 不可用于解密，跳过
    let code: u32 = digits.parse().ok()?;
    if code == 0 {
        return None;
    }
    Some(code)
}

/// 规范化账号目录名为派生用标识。
///
/// - `wxid_*` 形式：去掉末尾 `_<4hex>` 段
/// - 其他形式：仅当末尾恰为 `_<4hex>` 时去后缀；否则保持完整目录名
pub fn normalize_account_id(dir_name: &str) -> String {
    if dir_name.is_empty() {
        return dir_name.to_string();
    }
    // 末尾 _<4hex> 模式
    if let Some((prefix, suffix)) = dir_name.rsplit_once('_') {
        if suffix.len() == 4 && suffix.chars().all(|c| c.is_ascii_hexdigit()) && !prefix.is_empty()
        {
            // wxid_* 或其他带四位十六进制后缀的账号目录：去后缀
            return prefix.to_string();
        }
    }
    dir_name.to_string()
}

/// 派生账号密钥材料：AES key = MD5(decimal(code) || account_id) 小写 hex 前 16 ASCII 字节
pub fn derive_key_material(code: u32, account_dir_name: &str) -> AccountKeyMaterial {
    let account_id = normalize_account_id(account_dir_name);
    let mut hasher = Md5::new();
    hasher.update(code.to_string().as_bytes());
    hasher.update(account_id.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{:x}", digest);
    let mut aes_key = [0u8; 16];
    aes_key.copy_from_slice(&hex.as_bytes()[..16]);
    AccountKeyMaterial {
        account_id,
        code,
        aes_key,
        xor_byte: (code & 0xff) as u8,
    }
}

/// 为指定账号目录构建有限候选密钥列表。
///
/// 输入 code 集合（来自白名单文件名）× 账号标识变体，去重后设数量上限。
pub fn build_key_candidates(
    codes: &[u32],
    account_dir_name: &str,
    max_candidates: usize,
) -> Vec<AccountKeyMaterial> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for &code in codes {
        let material = derive_key_material(code, account_dir_name);
        let key_id = (material.code, material.account_id.clone());
        if seen.insert(key_id) {
            out.push(material);
        }
        if out.len() >= max_candidates {
            break;
        }
    }
    out
}

/// 将参数扫描错误映射为用户可读的领域错误
pub fn map_parameter_error(err: ParameterScanError) -> ChatVaultError {
    match err {
        ParameterScanError::DirectoryUnavailable => {
            ChatVaultError::WeChatParse("media_parameters_unavailable".into())
        }
        ParameterScanError::NoCodesFound => {
            ChatVaultError::WeChatParse("media_parameters_unavailable".into())
        }
        ParameterScanError::CandidateLimitExceeded => {
            ChatVaultError::WeChatParse("candidate_limit_exceeded".into())
        }
    }
}

/// 读取默认白名单目录中的 code 并为账号构建候选
pub fn prepare_account_candidates(
    account_dir_name: &str,
    custom_dirs: Option<&[PathBuf]>,
    max_candidates: usize,
) -> Result<Vec<AccountKeyMaterial>> {
    let dirs: Vec<PathBuf> = match custom_dirs {
        Some(d) => d.to_vec(),
        None => default_kvcomm_dirs(),
    };
    let codes = scan_parameter_codes(&dirs).map_err(map_parameter_error)?;
    if codes.len() > max_candidates {
        return Err(ChatVaultError::WeChatParse(
            "candidate_limit_exceeded".into(),
        ));
    }
    Ok(build_key_candidates(
        &codes,
        account_dir_name,
        max_candidates,
    ))
}

/// 判断路径是否位于允许的参数根目录内
pub fn is_within_parameter_roots(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_code_from_statistic_name() {
        assert_eq!(
            parse_code_from_statistic_name("key_123456789_abc.statistic"),
            Some(123456789)
        );
        assert_eq!(
            parse_code_from_statistic_name("key_0_logout.statistic"),
            None
        );
        assert_eq!(parse_code_from_statistic_name("other_file.statistic"), None);
        assert_eq!(parse_code_from_statistic_name("key_99.txt"), None);
    }

    #[test]
    fn normalizes_account_id_with_hex_suffix() {
        assert_eq!(
            normalize_account_id("wxid_abc123def_a1b2"),
            "wxid_abc123def"
        );
        assert_eq!(normalize_account_id("wxid_abc123def"), "wxid_abc123def");
        assert_eq!(normalize_account_id("user_abcd"), "user");
        assert_eq!(normalize_account_id("plainname"), "plainname");
    }

    #[test]
    fn derives_deterministic_key() {
        let a = derive_key_material(42, "wxid_test_a1b2");
        let b = derive_key_material(42, "wxid_test_a1b2");
        assert_eq!(a.aes_key, b.aes_key);
        assert_eq!(a.xor_byte, b.xor_byte);
        assert_eq!(a.aes_key.len(), 16);
        assert_eq!(a.xor_byte, 42u32 as u8);
        // 不同 code 得到不同 key
        let c = derive_key_material(43, "wxid_test_a1b2");
        assert_ne!(a.aes_key, c.aes_key);
    }

    #[test]
    fn scans_codes_from_temp_dir() {
        let dir = std::env::temp_dir().join(format!("cv_kvcomm_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("key_111_aaa.statistic"), b"x").unwrap();
        std::fs::write(dir.join("key_222_bbb.statistic"), b"x").unwrap();
        std::fs::write(dir.join("key_0_ccc.statistic"), b"x").unwrap();
        std::fs::write(dir.join("readme.txt"), b"x").unwrap();

        let codes = scan_parameter_codes(std::slice::from_ref(&dir)).unwrap();
        assert_eq!(codes, vec![111, 222]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_dir_reports_unavailable() {
        let missing = std::env::temp_dir().join("cv_kvcomm_missing_nonexistent");
        assert_eq!(
            scan_parameter_codes(&[missing]),
            Err(ParameterScanError::DirectoryUnavailable)
        );
    }
}
