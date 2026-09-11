// ChatVault 不可变资源写入：条件创建并完整回读，拒绝覆盖不一致的日志或提交。
use crate::WebDavClient;
use chatvault_core::error::{ChatVaultError, Result};
use reqwest::{Method, StatusCode};

impl WebDavClient {
    /// 创建不可变元数据；并发或重试仅接受完全相同的已有字节。
    pub async fn upload_immutable(&self, bytes: &[u8], path: &str) -> Result<()> {
        if let Some((parent, _)) = path.rsplit_once('/') {
            self.ensure_dir(parent).await?;
        }
        if !self.exists(path).await? {
            let response = self
                .build_request(Method::PUT, &self.get_full_url(path)?)
                .header("If-None-Match", "*")
                .body(bytes.to_vec())
                .send()
                .await
                .map_err(|e| ChatVaultError::WebDav(e.to_string()))?;
            if !response.status().is_success()
                && response.status() != StatusCode::PRECONDITION_FAILED
            {
                return Err(ChatVaultError::WebDav(format!(
                    "不可变资源写入失败: {}",
                    response.status()
                )));
            }
        }
        let actual = self
            .get_stream(path)
            .await?
            .bytes()
            .await
            .map_err(|e| ChatVaultError::WebDav(e.to_string()))?;
        if actual.as_ref() != bytes {
            return Err(ChatVaultError::WebDav(format!(
                "不可变资源已存在但内容不一致: {path}"
            )));
        }
        Ok(())
    }

    /// 返回规范化的连接根地址，作为索引绑定的一部分。
    pub fn storage_identity(&self) -> &str {
        self.config.base_url.trim_end_matches('/')
    }
}
