//! # WebDAV 服务能力探测模块
//!
//! 在用户首次配置 WebDAV 或启动归档前，探测远端服务器连通性及核心动词支持情况
//! （包括 HEAD, MKCOL, PUT, GET, MOVE 等），生成兼容性检测报告。

use crate::client::WebDavClient;
use chatvault_core::error::Result;
use serde::{Deserialize, Serialize};

/// WebDAV 服务能力探测报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityReport {
    /// 基础网络连通性
    pub reachable: bool,
    /// 认证是否通过
    pub authenticated: bool,
    /// 目录创建能力 (MKCOL)
    pub support_mkcol: bool,
    /// 暂存移动能力 (MOVE)
    pub support_move: bool,
    /// 探测过程中记录的详细说明或错误信息
    pub message: String,
}

/// 能力探测执行器
pub struct CapabilityDetector<'a> {
    client: &'a WebDavClient,
}

impl<'a> CapabilityDetector<'a> {
    /// 创建探测器
    ///
    /// 职责: 绑定 WebDavClient
    /// 输入: `client`: 客户端引用
    pub fn new(client: &'a WebDavClient) -> Self {
        Self { client }
    }

    /// 执行全套能力探测
    ///
    /// 职责: 依次测试 MKCOL、PUT、HEAD、GET 回读、PROPFIND、MOVE 与清理
    /// 输出: `Result<CapabilityReport>`
    pub async fn detect(&self) -> Result<CapabilityReport> {
        let test_root = format!(
            "ChatVault_Capability_Probe_{}",
            uuid::Uuid::new_v4().simple()
        );
        let src = format!("{test_root}/source");
        let dest = format!("{test_root}/destination");
        if let Err(e) = self.client.mkcol(&test_root).await {
            return Ok(CapabilityReport {
                reachable: false,
                authenticated: false,
                support_mkcol: false,
                support_move: false,
                message: format!("连接或 MKCOL 检测失败: {e}"),
            });
        }
        let payload = b"ChatVault Probe Payload";
        let hash = blake3::hash(payload).to_hex().to_string();
        // 分步记录：避免前面步骤失败时把未执行的 MOVE 误标为失败。
        let mut authenticated = false;
        let mut support_move = false;
        let probe = async {
            self.client.upload_bytes(payload.to_vec(), &src).await?;
            if !self.client.exists(&src).await? {
                return Err(chatvault_core::error::ChatVaultError::WebDav(
                    "上传后 HEAD 未找到对象".into(),
                ));
            }
            crate::RemoteVerifier::new(self.client)
                .verify_remote_hash(&src, &hash)
                .await?;
            authenticated = true;
            if !self.client.list_dir(&test_root).await?.contains(&src) {
                return Err(chatvault_core::error::ChatVaultError::WebDav(
                    "PROPFIND 未返回测试文件".into(),
                ));
            }
            self.client.move_resource(&src, &dest, false).await?;
            crate::RemoteVerifier::new(self.client)
                .verify_remote_hash(&dest, &hash)
                .await?;
            support_move = true;
            Ok::<(), chatvault_core::error::ChatVaultError>(())
        }
        .await;
        // 只清理本次随机探测目录，不触及用户 Vault。
        let _ = self.client.delete_resource(&src).await;
        let _ = self.client.delete_resource(&dest).await;
        let _ = self.client.delete_resource(&test_root).await;
        Ok(CapabilityReport {
            reachable: true,
            authenticated,
            support_mkcol: true,
            support_move,
            message: match probe {
                Ok(()) => "WebDAV 创建、上传、列举、移动和完整回读检测通过".into(),
                Err(e) => format!("WebDAV 能力检测未通过: {e}"),
            },
        })
    }
}
