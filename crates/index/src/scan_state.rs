// ChatVault 扫描检查点：记录扫描根上次开始时间，并提供已知文件内存比对与路径是否最新。
use crate::db::*;
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::models::DiscoveredFile;
use chatvault_core::paths::{is_under_root, normalize_root_path, normalize_scan_key};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::params;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 已索引本机文件的内存快照，供增量扫描跳过稳定/哈希
#[derive(Debug, Clone)]
pub struct KnownLocalFile {
    pub original_path: String,
    pub size: i64,
    pub mtime_ms: i64,
    pub cache_path: Option<String>,
}

impl KnownLocalFile {
    /// 将磁盘上已变化的已知文件转回待入库候选
    pub fn to_discovered(
        &self,
        source_type: &str,
        account_id: Option<&str>,
        conversation_hint: Option<String>,
    ) -> Option<DiscoveredFile> {
        let path = Path::new(&self.original_path);
        let file_name = path.file_name()?.to_str()?.to_string();
        let meta = std::fs::metadata(path).ok()?;
        let modified_time: DateTime<Utc> = meta.modified().ok()?.into();
        Some(DiscoveredFile {
            source_type: source_type.to_string(),
            account_id: account_id.map(|s| s.to_string()),
            absolute_path: self.original_path.clone(),
            file_name,
            file_size: meta.len(),
            modified_time,
            conversation_hint,
        })
    }
}

/// 将毫秒时间戳转为 SystemTime，用于目录 mtime 剪枝
pub fn system_time_from_ms(ms: i64) -> SystemTime {
    if ms <= 0 {
        return UNIX_EPOCH;
    }
    UNIX_EPOCH + Duration::from_millis(ms as u64)
}

/// 将 SystemTime 转为毫秒时间戳
pub fn system_time_to_ms(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Database {
    /// 读取扫描根上次扫描开始时间（毫秒）；无记录表示尚未建立检查点，应全量发现
    pub fn get_scan_started_ms(&self, root_path: &str) -> Result<Option<i64>> {
        let key = normalize_root_path(root_path);
        let mut stmt = self
            .conn
            .prepare("SELECT last_scan_started_ms FROM scan_roots WHERE root_path = ?1")
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let value = stmt
            .query_row(params![key], |row| row.get::<_, i64>(0))
            .ok();
        Ok(value)
    }

    /// 记录扫描根检查点。应写入「本轮扫描开始时刻」，使扫描过程中新建的文件下轮仍能被发现。
    pub fn mark_scan_started(
        &mut self,
        root_path: &str,
        source_kind: &str,
        account_id: Option<&str>,
        started_ms: i64,
    ) -> Result<()> {
        let key = normalize_root_path(root_path);
        self.conn
            .execute(
                "INSERT INTO scan_roots (root_path, source_kind, account_id, last_scan_started_ms)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(root_path) DO UPDATE SET
                   source_kind = excluded.source_kind,
                   account_id = excluded.account_id,
                   last_scan_started_ms = excluded.last_scan_started_ms",
                params![key, source_kind, account_id, started_ms],
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        Ok(())
    }

    /// 加载扫描根下全部已索引路径，供增量遍历做 O(1) 内存比对
    pub fn list_known_files_under(
        &self,
        root_path: &str,
    ) -> Result<HashMap<String, KnownLocalFile>> {
        let root_key = normalize_root_path(root_path);
        let mut stmt = self
            .conn
            .prepare(
                "SELECT original_path, size, mtime_ms, cache_path
                 FROM local_files
                 ORDER BY rowid DESC",
            )
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(KnownLocalFile {
                    original_path: row.get(0)?,
                    size: row.get(1)?,
                    mtime_ms: row.get(2)?,
                    cache_path: row.get(3)?,
                })
            })
            .map_err(|e| ChatVaultError::Database(e.to_string()))?;

        let mut map = HashMap::new();
        for row in rows {
            let known = row.map_err(|e| ChatVaultError::Database(e.to_string()))?;
            if is_under_root(&known.original_path, &root_key) {
                // 同一路径可能因文件内容变化产生多条历史记录，只保留最新插入的记录。
                map.entry(normalize_scan_key(&known.original_path))
                    .or_insert(known);
            }
        }
        Ok(map)
    }

    /// 判断磁盘上的文件是否与索引一致（mtime/size 且缓存策略满足），无需读内容
    ///
    /// 职责: 与 `ingest_file` 的跳过条件对齐，供编排层只对变更文件做稳定性检测
    /// 输出: true 表示可直接跳过；false 表示需要入库流程
    pub fn path_is_current(&self, abs_path: &str) -> Result<bool> {
        // 与 ingest 相同的路径规范化，保证能命中 local_files
        let canonical = match std::fs::canonicalize(abs_path) {
            Ok(p) => p,
            Err(_) => return Ok(false),
        };
        let stored = {
            let s = canonical.to_string_lossy();
            if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
                format!(r"\\{rest}")
            } else if let Some(rest) = s.strip_prefix(r"\\?\") {
                rest.to_string()
            } else {
                s.to_string()
            }
        };
        let key = normalize_scan_key(&stored);
        let known = {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT original_path, size, mtime_ms, cache_path FROM local_files
                     WHERE original_path = ?1
                     ORDER BY rowid DESC",
                )
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![stored], |row| {
                    Ok(KnownLocalFile {
                        original_path: row.get(0)?,
                        size: row.get(1)?,
                        mtime_ms: row.get(2)?,
                        cache_path: row.get(3)?,
                    })
                })
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            let mut found = None;
            for row in rows {
                let known = row.map_err(|e| ChatVaultError::Database(e.to_string()))?;
                if normalize_scan_key(&known.original_path) == key {
                    found = Some(known);
                    break;
                }
            }
            found
        };

        let Some(known) = known else {
            return Ok(false);
        };

        let meta = match std::fs::metadata(abs_path) {
            Ok(m) => m,
            Err(_) => return Ok(false),
        };
        let size = meta.len() as i64;
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        if mtime_ms != known.mtime_ms || size != known.size {
            return Ok(false);
        }
        if let Some(cache) = &known.cache_path {
            if !std::path::Path::new(cache).exists() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// 从已知文件中筛出磁盘 mtime/size 已变化、需要重新入库的条目
    pub fn list_changed_known_files(&self, root_path: &str) -> Result<Vec<KnownLocalFile>> {
        let known = self.list_known_files_under(root_path)?;
        let mut changed = Vec::new();
        for item in known.into_values() {
            let meta = match std::fs::metadata(&item.original_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let size = meta.len() as i64;
            let mtime_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let cache_ok = item
                .cache_path
                .as_ref()
                .map(|p| std::path::Path::new(p).exists())
                .unwrap_or(true);
            if mtime_ms != item.mtime_ms || size != item.size || !cache_ok {
                changed.push(item);
            }
        }
        Ok(changed)
    }
}

/// 将 chrono 时间转为毫秒时间戳
pub fn chrono_ms(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}

/// 从本地时间戳构造 Utc
pub fn utc_from_ms(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatvault_core::models::DiscoveredFile;
    use std::fs::File;
    use std::io::Write;
    use uuid::Uuid;

    fn temp_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn write_file(dir: &std::path::Path, name: &str, content: &[u8]) -> std::path::PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        let mut f = File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn test_scan_checkpoint_roundtrip() {
        let mut db = temp_db();
        let root = std::env::temp_dir().join(format!("cv_scan_root_{}", Uuid::new_v4()));
        let root_s = root.to_string_lossy().to_string();
        assert!(db.get_scan_started_ms(&root_s).unwrap().is_none());
        db.mark_scan_started(&root_s, "generic-folder", None, 1_700_000_000_000)
            .unwrap();
        assert_eq!(
            db.get_scan_started_ms(&root_s).unwrap(),
            Some(1_700_000_000_000)
        );
        db.mark_scan_started(&root_s, "generic-folder", None, 1_700_000_001_000)
            .unwrap();
        assert_eq!(
            db.get_scan_started_ms(&root_s).unwrap(),
            Some(1_700_000_001_000)
        );
    }

    #[test]
    fn test_path_is_current_and_changed_list() {
        let mut db = temp_db();
        let dir = std::env::temp_dir().join(format!("cv_scan_files_{}", Uuid::new_v4()));
        let path = write_file(&dir, "a.txt", b"hello");
        let df = DiscoveredFile {
            source_type: "generic-folder".into(),
            account_id: None,
            absolute_path: path.to_string_lossy().to_string(),
            file_name: "a.txt".into(),
            file_size: 5,
            modified_time: Utc::now(),
            conversation_hint: None,
        };
        db.ingest_file(&df, "dev-test").unwrap();
        let abs = path.to_string_lossy().to_string();
        assert!(db.path_is_current(&abs).unwrap());

        // 改内容与 mtime
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut f = File::create(&path).unwrap();
        f.write_all(b"hello-world").unwrap();
        drop(f);
        assert!(!db.path_is_current(&abs).unwrap());
        let changed = db.list_changed_known_files(&dir.to_string_lossy()).unwrap();
        assert_eq!(changed.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_system_time_roundtrip() {
        let now_ms = 1_723_000_000_123i64;
        let t = system_time_from_ms(now_ms);
        assert_eq!(system_time_to_ms(t), now_ms);
    }
}
