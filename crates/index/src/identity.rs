// ChatVault 本机身份和远端绑定：禁止多个资料库共用同步游标。
use crate::Database;
use chatvault_core::error::{ChatVaultError, Result};

/// 清空旧 Vault 后的状态摘要
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultResetReport {
    /// 是否原本存在远端绑定
    pub had_binding: bool,
    /// 清除的同步游标数
    pub cleared_cursors: usize,
    /// 清除的本机日志事件数
    pub cleared_journal_events: usize,
    /// 清除的已应用远端事件数
    pub cleared_applied_events: usize,
    /// 清除的已知设备数
    pub cleared_devices: usize,
    /// 重置为待上传的任务数
    pub requeued_uploads: usize,
    /// 清除的同步相关设置项数（remote_binding / pending_segment_* / epoch_*）
    pub cleared_settings: usize,
}

impl Database {
    /// 首次生成并持久化唯一设备身份；已有身份经校验后直接复用。
    pub fn ensure_device_identity(&mut self) -> Result<String> {
        self.atomic(|db| {
            db.conn
                .execute(
                    "UPDATE app_settings SET value=value WHERE key='device_id'",
                    [],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            if let Some(id) = db.get_setting("device_id")? {
                chatvault_metadata::validate_id(&id)?;
                return Ok(id);
            }
            let id = uuid::Uuid::new_v4().to_string();
            db.set_setting("device_id", &id)?;
            Ok(id)
        })
    }

    /// 读取当前远端绑定（规范化 URL 与 Vault ID）
    ///
    /// 输出: `Ok(None)` 表示尚未绑定；`Ok(Some((url, vault)))` 为已关联目标。
    pub fn get_remote_binding(&self) -> Result<Option<(String, String)>> {
        let Some(raw) = self.get_setting("remote_binding")? else {
            return Ok(None);
        };
        let (url, vault): (String, String) = serde_json::from_str(&raw)
            .map_err(|e| ChatVaultError::Database(format!("解析远端绑定失败: {}", e)))?;
        Ok(Some((url, vault)))
    }

    /// 校验设置或操作目标是否与已绑定资料库一致；不修改状态。
    pub fn check_remote_binding(&self, url: &str, vault: &str) -> Result<()> {
        chatvault_metadata::validate_vault_id(vault)?;
        if let Some(bound) = self.get_setting("remote_binding")? {
            let target = serde_json::to_string(&(url.trim().trim_end_matches('/'), vault))?;
            if bound != target {
                return Err(ChatVaultError::Internal("当前已绑定其他资料库；若要换用新的 Vault ID，请先在设置里清空旧 Vault".into()));
            }
        }
        Ok(())
    }

    /// 首次远端操作前持久化绑定；事务阻止并发命令绑定到不同目标。
    pub fn bind_remote(&mut self, url: &str, vault: &str) -> Result<()> {
        self.atomic(|db| {
            // 先取得写锁，避免两个首次连接同时通过检查。
            db.conn
                .execute(
                    "UPDATE app_settings SET value=value WHERE key='remote_binding'",
                    [],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            db.check_remote_binding(url, vault)?;
            db.set_setting(
                "remote_binding",
                &serde_json::to_string(&(url.trim().trim_end_matches('/'), vault))?,
            )
        })
    }

    /// 清空与旧 Vault 绑定相关的同步状态，允许改绑新 Vault。
    ///
    /// 职责: 解除远端绑定并清除同步游标/日志/已应用事件/设备注册；本地文件索引与来源映射保留。
    /// 输出: `VaultResetReport`，汇总本次清理数量。
    pub fn reset_vault_binding(&mut self) -> Result<VaultResetReport> {
        self.atomic(|db| {
            db.conn
                .execute(
                    "UPDATE app_settings SET value=value WHERE key='remote_binding'",
                    [],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            let had_binding = db.get_setting("remote_binding")?.is_some();

            let cleared_settings = db
                .conn
                .execute(
                    "DELETE FROM app_settings
                     WHERE key = 'remote_binding'
                        OR key LIKE 'pending\\_segment\\_%' ESCAPE '\\'
                        OR key LIKE 'epoch\\_%' ESCAPE '\\'",
                    [],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            let cleared_cursors = db
                .conn
                .execute("DELETE FROM sync_cursors", [])
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            let cleared_journal_events = db
                .conn
                .execute("DELETE FROM journal_events", [])
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            let cleared_applied_events = db
                .conn
                .execute("DELETE FROM applied_events", [])
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            let cleared_devices = db
                .conn
                .execute("DELETE FROM known_devices", [])
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            // 新 Vault 尚无远端对象，原 backed_up/paused 之外的任务全部重新排队归档。
            let requeued_uploads = db
                .conn
                .execute(
                    "UPDATE upload_tasks
                     SET status = 'queued', error_message = NULL, retry_count = 0, updated_at = ?1
                     WHERE status != 'paused'",
                    [chrono::Utc::now().to_rfc3339()],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;

            Ok(VaultResetReport {
                had_binding,
                cleared_cursors,
                cleared_journal_events,
                cleared_applied_events,
                cleared_devices,
                requeued_uploads,
                cleared_settings,
            })
        })
    }
}
