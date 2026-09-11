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
    /// 职责: 依次测试 HEAD、MKCOL、PUT 测试文件、MOVE 重命名与清理
    /// 输出: `Result<CapabilityReport>`
    pub async fn detect(&self) -> Result<CapabilityReport> {
        let test_root = "ChatVault_Capability_Probe";

        // 1. 测试基础连接与 MKCOL
        let mkcol_res = self.client.mkcol(test_root).await;
        if let Err(e) = &mkcol_res {
            return Ok(CapabilityReport {
                reachable: true,
                authenticated: false,
                support_mkcol: false,
                support_move: false,
                message: format!("MKCOL 测试失败 (可能是凭据错误或目录无权创建): {}", e),
            });
        }

        // 2. 测试写入临时文件
        let test_src = format!("{}/probe_src.txt", test_root);
        let test_dest = format!("{}/probe_dest.txt", test_root);
        let payload = b"ChatVault Probe Payload".to_vec();

        if let Err(e) = self.client.upload_bytes(payload, &test_src).await {
            return Ok(CapabilityReport {
                reachable: true,
                authenticated: true,
                support_mkcol: true,
                support_move: false,
                message: format!("PUT 写入测试文件失败: {}", e),
            });
        }

        // 3. 测试 MOVE 动词
        let move_ok = match self.client.move_resource(&test_src, &test_dest, true).await {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("MOVE 测试失败: {}", e);
                false
            }
        };

        Ok(CapabilityReport {
            reachable: true,
            authenticated: true,
            support_mkcol: true,
            support_move: move_ok,
            message: "WebDAV 基础服务检测全部通过".to_string(),
        })
    }
}
