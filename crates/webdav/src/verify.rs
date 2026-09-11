//! # 远端回读与内容哈希校验模块
//!
//! 实现远端归档验证：通过 GET 流式读取远端已上传的文件内容，
//! 实时计算 BLAKE3 哈希并与预期哈希进行逐位比对，确保远端文件无损存储。

use crate::client::WebDavClient;
use chatvault_core::error::{ChatVaultError, Result};
use futures_util::StreamExt;

/// 远端回读哈希校验器
pub struct RemoteVerifier<'a> {
    client: &'a WebDavClient,
}

impl<'a> RemoteVerifier<'a> {
    /// 创建校验器实例
    ///
    /// 职责: 绑定 WebDAV 客户端
    /// 输入: `client`: WebDavClient 引用
    pub fn new(client: &'a WebDavClient) -> Self {
        Self { client }
    }

    /// 执行远端回读并比对 BLAKE3 哈希
    ///
    /// 职责: 流式拉取远端文件数据，计算其 BLAKE3 并比对预期哈希
    /// 输入:
    ///   - `remote_path`: 远端相对路径 (例如 `ChatVault/<vault_id>/objects/blake3/...`)
    ///   - `expected_hex`: 本地计算并预期的十六进制哈希
    /// 输出: `Result<()>`
    /// 关键约束:
    ///   - 必须全量回读整个流，若传输中断或哈希不匹配，返回明确错误
    pub async fn verify_remote_hash(&self, remote_path: &str, expected_hex: &str) -> Result<()> {
        self.verify_remote_size(remote_path, expected_hex)
            .await
            .map(|_| ())
    }

    /// 完整验证哈希并返回实际读取大小，供同步层验证元数据长度。
    pub async fn verify_remote_size(&self, remote_path: &str, expected_hex: &str) -> Result<u64> {
        let clean_expected = expected_hex.trim_start_matches("blake3:");
        let resp = self.client.get_stream(remote_path).await?;
        let mut stream = resp.bytes_stream();

        let mut hasher = blake3::Hasher::new();
        let mut total_bytes = 0u64;

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res
                .map_err(|e| ChatVaultError::WebDav(format!("回读远端文件流中断: {}", e)))?;
            hasher.update(&chunk);
            total_bytes += chunk.len() as u64;
        }

        let actual_hex = hasher.finalize().to_hex().to_string();

        if actual_hex.eq_ignore_ascii_case(clean_expected) {
            tracing::info!(
                "远端文件校验通过: 路径={}, 大小={} bytes, 哈希={}",
                remote_path,
                total_bytes,
                actual_hex
            );
            Ok(total_bytes)
        } else {
            tracing::error!(
                "远端哈希校验不匹配! 路径={}, 预期={}, 实际={}",
                remote_path,
                clean_expected,
                actual_hex
            );
            Err(ChatVaultError::HashMismatch {
                expected: clean_expected.to_string(),
                actual: actual_hex,
            })
        }
    }
}
