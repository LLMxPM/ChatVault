// ChatVault 索引子模块：封装持久化操作与事务边界。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};
use chrono::Utc;
use rusqlite::params;

impl Database {
    /// 读取应用设置项
    ///
    /// 输入: `key`: 设置键名
    /// 输出: `Result<Option<String>>`
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM app_settings WHERE key = ?1")
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let value = stmt
            .query_row(params![key], |row| row.get::<_, String>(0))
            .ok();
        Ok(value)
    }

    /// 写入应用设置项（存在则覆盖）
    ///
    /// 输入:
    ///   - `key`: 设置键名
    ///   - `value`: 设置值
    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value, now],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 批量读取全部设置项为键值对
    pub fn get_all_settings(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM app_settings")
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (k, v) = row.map_err(|e| ChatVaultError::Database(e.to_string()))?;
            map.insert(k, v);
        }
        Ok(map)
    }
}
