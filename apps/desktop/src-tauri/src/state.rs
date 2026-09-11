// ChatVault 桌面端全局状态管理
// 从本地 SQLite 读取设备身份、Vault 配置与用户设置，支持运行时更新

use chatvault_core::error::Result;
use chatvault_index::Database;
use std::path::PathBuf;

/// 设置键名常量
pub mod setting_keys {
    pub const VAULT_ID: &str = "vault_id";
    pub const DEVICE_ID: &str = "device_id";
    pub const WEBDAV_URL: &str = "webdav_url";
    pub const WEBDAV_USERNAME: &str = "webdav_username";
    pub const SCAN_INTERVAL_MINUTES: &str = "scan_interval_minutes";
    pub const SCHEDULE_ENABLED: &str = "schedule_enabled";
    pub const COLLECT_DIRS: &str = "collect_dirs";
}

/// 桌面端全局应用上下文状态
#[derive(Clone)]
pub struct AppState {
    /// 本地 SQLite 数据库文件路径
    pub db_path: PathBuf,
}

impl AppState {
    /// 创建并初始化应用状态，首次启动写入默认 vault/device 设置
    pub fn new(
        db_path: PathBuf,
        default_vault_id: String,
        _default_device_id: String,
    ) -> Result<Self> {
        let mut db = Database::open(&db_path)?;

        if db.get_setting(setting_keys::VAULT_ID)?.is_none() {
            db.set_setting(setting_keys::VAULT_ID, &default_vault_id)?;
        }
        db.ensure_device_identity()?;
        if db
            .get_setting(setting_keys::SCAN_INTERVAL_MINUTES)?
            .is_none()
        {
            db.set_setting(setting_keys::SCAN_INTERVAL_MINUTES, "30")?;
        }
        if db.get_setting(setting_keys::SCHEDULE_ENABLED)?.is_none() {
            db.set_setting(setting_keys::SCHEDULE_ENABLED, "false")?;
        }

        Ok(Self { db_path })
    }

    /// 获取新的数据库连接句柄
    pub fn get_db(&self) -> Result<Database> {
        Database::open(&self.db_path)
    }

    /// 读取当前设备标识
    #[allow(dead_code)]
    pub fn device_id(&self) -> Result<String> {
        self.get_db()?.ensure_device_identity()
    }

    /// 读取当前 Vault 标识
    pub fn vault_id(&self) -> Result<String> {
        let db = self.get_db()?;
        Ok(db
            .get_setting(setting_keys::VAULT_ID)?
            .unwrap_or_else(|| "default-vault".to_string()))
    }
}
