// ChatVault 桌面端全局状态管理
// 从本地 SQLite 读取设备身份、Vault 配置与用户设置，支持运行时更新

use chatvault_core::error::Result;
use chatvault_index::Database;
use chatvault_metadata::VAULT_ID_PREFIX;
use std::path::PathBuf;

/// 设置键名常量
pub mod setting_keys {
    pub const VAULT_ID: &str = "vault_id";
    pub const DEVICE_ID: &str = "device_id";
    pub const DEVICE_NAME: &str = "device_name";
    pub const WEBDAV_URL: &str = "webdav_url";
    pub const WEBDAV_USERNAME: &str = "webdav_username";
    pub const SCAN_INTERVAL_MINUTES: &str = "scan_interval_minutes";
    pub const SCHEDULE_ENABLED: &str = "schedule_enabled";
    pub const COLLECT_SOURCES: &str = "collect_sources";
    /// 采集源上次探测快照：路径 → 状态/账号，用于任务页首屏秒开
    pub const COLLECT_SOURCE_CACHE: &str = "collect_source_cache";
    /// 立即运行时勾选的微信账号；None 表示尚未配置（默认全选）
    pub const COLLECT_SELECTED_ACCOUNTS: &str = "collect_selected_accounts";
    pub const DOWNLOAD_DIR: &str = "download_dir";
}

/// 默认 Vault ID：`chatvault-default`
pub fn default_vault_id() -> String {
    format!("{VAULT_ID_PREFIX}default")
}

/// 默认设备名称：优先 Windows 计算机名，否则回退「本机」
pub fn default_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("HOSTNAME")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "本机".to_string())
}

/// 桌面端全局应用上下文状态
#[derive(Clone)]
pub struct AppState {
    /// 本地 SQLite 数据库文件路径
    pub db_path: PathBuf,
}

impl AppState {
    /// 创建并初始化应用状态，首次启动写入默认 vault/device 设置
    pub fn new(db_path: PathBuf, default_vault_id: String) -> Result<Self> {
        let mut db = Database::open(&db_path)?;

        if db.get_setting(setting_keys::VAULT_ID)?.is_none() {
            db.set_setting(setting_keys::VAULT_ID, &default_vault_id)?;
        }
        db.ensure_device_identity()?;
        if db.get_setting(setting_keys::DEVICE_NAME)?.is_none() {
            db.set_setting(setting_keys::DEVICE_NAME, &default_device_name())?;
        }
        if db
            .get_setting(setting_keys::SCAN_INTERVAL_MINUTES)?
            .is_none()
        {
            db.set_setting(setting_keys::SCAN_INTERVAL_MINUTES, "30")?;
        }
        if db.get_setting(setting_keys::SCHEDULE_ENABLED)?.is_none() {
            db.set_setting(setting_keys::SCHEDULE_ENABLED, "false")?;
        }
        if db.get_setting(setting_keys::COLLECT_SOURCES)?.is_none() {
            db.set_setting(setting_keys::COLLECT_SOURCES, "[]")?;
        }
        if db
            .get_setting(setting_keys::COLLECT_SOURCE_CACHE)?
            .is_none()
        {
            db.set_setting(setting_keys::COLLECT_SOURCE_CACHE, "[]")?;
        }

        // 仅启动时清理一次；后续 get_db 不得再做此维护，否则会中断进行中的流水线。
        db.maintain_task_runs_on_startup()?;

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

    /// 读取当前设备名称；空值时回退默认名称
    pub fn device_name(&self) -> Result<String> {
        let db = self.get_db()?;
        Ok(db
            .get_setting(setting_keys::DEVICE_NAME)?
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(default_device_name))
    }

    /// 读取当前 Vault 标识
    pub fn vault_id(&self) -> Result<String> {
        let db = self.get_db()?;
        Ok(db
            .get_setting(setting_keys::VAULT_ID)?
            .unwrap_or_else(default_vault_id))
    }
}
