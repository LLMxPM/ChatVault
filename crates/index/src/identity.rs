// ChatVault 本机身份和远端绑定：禁止多个资料库共用同步游标。
use crate::Database;
use chatvault_core::error::{ChatVaultError, Result};

impl Database {
    /// 首次生成唯一设备身份；旧固定身份的事件换用新事件 ID 重新发布，来源记录 ID 保持不变。
    pub fn ensure_device_identity(&mut self) -> Result<String> {
        self.atomic(|db| {
            db.conn
                .execute(
                    "UPDATE app_settings SET value=value WHERE key='device_id'",
                    [],
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            let old = db.get_setting("device_id")?;
            if let Some(ref id) = old {
                if !["win-pc-01", "windows-pc", "cli-device"].contains(&id.as_str())
                    && !id.is_empty()
                {
                    chatvault_metadata::validate_id(id)?;
                    return Ok(id.clone());
                }
            }
            let id = uuid::Uuid::new_v4().to_string();
            // 旧 CLI 没有 app_settings，扫描默认使用 windows-pc。
            let legacy = old.as_deref().unwrap_or("windows-pc");
            let events = db.list_unpublished_journal_events(legacy, 0, i64::MAX as u64)?;
            for (index, mut event) in events.into_iter().enumerate() {
                event.event_id = uuid::Uuid::new_v4().to_string();
                event.device_id = id.clone();
                event.epoch = 1;
                event.seq = index as u64 + 1;
                db.insert_journal_event_if_absent(&event)?;
            }
            db.set_setting("device_id", &id)?;
            Ok(id)
        })
    }

    /// 校验设置或操作目标是否与已绑定资料库一致；不修改状态。
    pub fn check_remote_binding(&self, url: &str, vault: &str) -> Result<()> {
        chatvault_metadata::validate_id(vault)?;
        if let Some(bound) = self.get_setting("remote_binding")? {
            let target = serde_json::to_string(&(url.trim().trim_end_matches('/'), vault))?;
            if bound != target {
                return Err(ChatVaultError::Internal("当前索引已绑定其他远端资料库；请为新 Vault 使用独立数据库，不能复用归档状态和同步游标".into()));
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
}
