// ChatVault WebDAV 两阶段原子归档发布器
// 严格践行暂存(staging) -> 回读校验(verify) -> 原子重命名(MOVE)发布模型，避免云端残留损坏对象

use crate::client::WebDavClient;
use crate::verify::RemoteVerifier;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_metadata::{get_object_path, get_staging_path};
use std::path::Path;
use uuid::Uuid;

/// 归档发布状态结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishResult {
    /// 远端已存在相同哈希的对象，已复用跳过上传
    AlreadyExists { final_path: String },
    /// 经过暂存、回读比对、原子移动成功发布
    Published {
        staging_path: String,
        final_path: String,
    },
}

/// 内容对象原子发布器
pub struct ObjectPublisher<'a> {
    client: &'a WebDavClient,
    verifier: RemoteVerifier<'a>,
}

impl<'a> ObjectPublisher<'a> {
    /// 创建原子发布器实例
    ///
    /// 职责: 绑定 WebDavClient 与回读校验器
    /// 输入: `client`: 客户端引用
    pub fn new(client: &'a WebDavClient) -> Self {
        Self {
            client,
            verifier: RemoteVerifier::new(client),
        }
    }

    /// 执行两阶段原子发布流程
    ///
    /// 职责:
    ///   1. 检查最终路径是否已存在；已存在则跳过上传直接返回
    ///   2. 上传文件到 staging 暂存目录
    ///   3. 回读 staging 文件并比对 BLAKE3 哈希
    ///   4. 校验通过后通过 HTTP MOVE 移动至最终 objects 路径
    ///   5. 若中间任何步骤失败，清理 staging 暂存文件
    ///
    /// 输入:
    ///   - `vault_id`: 资料库标识
    ///   - `device_id`: 发起归档的设备标识
    ///   - `local_path`: 本地源文件路径
    ///   - `expected_hash`: 预期的 BLAKE3 哈希值 (如 e3b0c44...)
    ///
    /// 输出: `Result<PublishResult>`
    pub async fn publish_object<P: AsRef<Path>>(
        &self,
        vault_id: &str,
        device_id: &str,
        local_path: P,
        expected_hash: &str,
    ) -> Result<PublishResult> {
        let lp = local_path.as_ref();
        if !lp.exists() {
            return Err(ChatVaultError::FileNotFound {
                path: lp.display().to_string(),
            });
        }

        let clean_hash = expected_hash.trim_start_matches("blake3:");
        chatvault_metadata::validate_vault_id(vault_id)?;
        chatvault_metadata::validate_id(device_id)?;
        chatvault_metadata::validate_hash(clean_hash)?;
        let final_path = get_object_path(vault_id, clean_hash);

        // 1. 检查最终对象是否已存在（实现跨设备/跨会话秒传与去重）
        if self.client.exists(&final_path).await? {
            self.verifier
                .verify_remote_hash(&final_path, clean_hash)
                .await?;
            tracing::info!("远端对象已存在，跳过上传直接复用: {}", final_path);
            return Ok(PublishResult::AlreadyExists { final_path });
        }

        // 2. 生成暂存唯一路径
        let upload_id = Uuid::new_v4().to_string();
        let staging_path = get_staging_path(vault_id, device_id, &upload_id);

        tracing::info!("正在上传到暂存路径: {} -> {}", lp.display(), staging_path);

        // 3. 上传文件到 staging 目录
        if let Err(e) = self.client.upload_file(lp, &staging_path).await {
            let _ = self.client.delete_resource(&staging_path).await;
            return Err(ChatVaultError::WebDav(format!("上传至暂存区失败: {}", e)));
        }

        // 4. 对暂存文件流式回读并验证 BLAKE3 哈希
        tracing::info!("正在对暂存文件执行流式回读校验: {}", staging_path);
        if let Err(e) = self
            .verifier
            .verify_remote_hash(&staging_path, clean_hash)
            .await
        {
            let _ = self.client.delete_resource(&staging_path).await;
            return Err(ChatVaultError::WebDav(format!(
                "暂存文件哈希校验失败，已自动清理暂存: {}",
                e
            )));
        }

        // 5. 校验通过，原子 MOVE 到最终路径
        tracing::info!(
            "暂存文件校验通过，执行原子重命名: {} -> {}",
            staging_path,
            final_path
        );
        if let Err(e) = self
            .client
            .move_resource(&staging_path, &final_path, false)
            .await
        {
            let _ = self.client.delete_resource(&staging_path).await;
            // 并发发布者可能已创建同一对象，仅在完整校验后复用。
            if self.client.exists(&final_path).await? {
                self.verifier
                    .verify_remote_hash(&final_path, clean_hash)
                    .await?;
                return Ok(PublishResult::AlreadyExists { final_path });
            }
            return Err(ChatVaultError::WebDav(format!(
                "MOVE 移动到最终路径失败: {}",
                e
            )));
        }

        // 6. MOVE 后复验最终对象可读且哈希一致
        if let Err(e) = self
            .verifier
            .verify_remote_hash(&final_path, clean_hash)
            .await
        {
            return Err(ChatVaultError::WebDav(format!(
                "MOVE 后最终对象校验失败: {}",
                e
            )));
        }

        Ok(PublishResult::Published {
            staging_path,
            final_path,
        })
    }
}
