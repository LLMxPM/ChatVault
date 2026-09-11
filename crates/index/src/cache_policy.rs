// ChatVault 本地副本策略：配置大小阈值、归档后保留时间与容量目标。
use crate::Database;
use chatvault_core::error::{ChatVaultError, Result};

pub const COPY_THRESHOLD_MIB: &str = "copy_threshold_mib";
pub const CACHE_RETENTION_DAYS: &str = "cache_retention_days";
pub const CACHE_MAX_MIB: &str = "cache_max_mib";
pub const MIB: u64 = 1024 * 1024;

/// 本机缓存策略；0 阈值禁用复制，0 天或 0 容量表示归档同步后立即回收。
pub struct CachePolicy {
    pub copy_threshold_mib: u32,
    pub cache_retention_days: u32,
    pub cache_max_mib: u32,
}

impl Database {
    /// 读取策略；未设置采用默认值，非法配置显式报错。
    pub fn cache_policy(&self) -> Result<CachePolicy> {
        let read = |key, default| -> Result<u32> {
            self.get_setting(key)?.map_or(Ok(default), |v| {
                v.parse()
                    .map_err(|_| ChatVaultError::Database(format!("无效缓存配置：{key}")))
            })
        };
        Ok(CachePolicy {
            copy_threshold_mib: read(COPY_THRESHOLD_MIB, 100)?,
            cache_retention_days: read(CACHE_RETENTION_DAYS, 7)?,
            cache_max_mib: read(CACHE_MAX_MIB, 1024)?,
        })
    }
}
