// ChatVault 缓存回收：保护未完成归档的引用，仅删除受控目录内的内容对象。
use crate::{cache_policy::MIB, Database};
use chatvault_core::error::{ChatVaultError, Result};
use chatvault_core::is_under_root_canonical;
use chatvault_core::models::CollectSource;
use std::{fs, path::Path, time::SystemTime};

/// 采集源设置键，与桌面端 setting_keys::COLLECT_SOURCES 一致。
const COLLECT_SOURCES_KEY: &str = "collect_sources";

impl Database {
    /// 清理崩溃后遗留的系统打开副本；只处理超过一天的普通文件，避免误删正在打开的内容。
    pub fn recover_open_cache(&mut self) -> Result<usize> {
        let now = SystemTime::now();
        let mut removed = 0usize;
        let directory = self.staging_dir.join("open");
        if !directory.is_dir() {
            return Ok(0);
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_file() || file_type.is_symlink() {
                continue;
            }
            let modified = entry.metadata()?.modified().unwrap_or(now);
            if now.duration_since(modified).unwrap_or_default()
                < std::time::Duration::from_secs(86_400)
            {
                continue;
            }
            if fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// 按保留时间和容量目标回收副本，返回释放字节数；不删除原附件。
    /// 与入库共用 SQLite 写锁，防止复制落盘与建立引用之间被回收。
    pub fn reclaim_cache(&mut self) -> Result<u64> {
        let policy = self.cache_policy()?;
        if !self.staging_dir.exists() {
            return Ok(0);
        }
        let directory = fs::canonicalize(&self.staging_dir)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let mut entries = Vec::new();
        let mut total = 0u64;
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            // 不跟随符号链接，不接触临时文件或未知文件。
            if !entry.file_type()?.is_file() || chatvault_metadata::validate_hash(&name).is_err() {
                continue;
            }
            let metadata = entry.metadata()?;
            total = total.saturating_add(metadata.len());
            let object = format!("blake3:{name}");
            let protected: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM local_files l JOIN file_records r USING(record_id)
                 WHERE r.object_id=?1 AND (
                   NOT EXISTS(SELECT 1 FROM upload_tasks t WHERE t.record_id=l.record_id AND t.status='backed_up')
                   OR EXISTS(SELECT 1 FROM upload_tasks t WHERE t.record_id=l.record_id AND t.status!='backed_up')
                   OR NOT EXISTS(SELECT 1 FROM journal_events j JOIN sync_cursors c ON c.device_id=j.device_id AND c.epoch=j.epoch
                     WHERE j.event_type='file_record_added' AND json_extract(j.payload,'$.record_id')=l.record_id AND c.last_contiguous_seq>=j.seq)
                 ))", [&object], |r| r.get(0)).map_err(db_error)?;
            if protected {
                continue;
            }
            let archived: Option<i64> = tx
                .query_row(
                    "SELECT MAX(unixepoch(updated_at)) FROM upload_tasks WHERE object_id=?1",
                    [&object],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            let timestamp = archived.map(|v| v.max(0) as u64).unwrap_or(
                metadata
                    .modified()?
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            entries.push((timestamp, entry.path(), object, metadata.len()));
        }
        entries.sort_by_key(|e| e.0);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut released = 0;
        for (timestamp, path, object, size) in entries {
            if now.saturating_sub(timestamp) < u64::from(policy.cache_retention_days) * 86400
                && total <= u64::from(policy.cache_max_mib) * MIB
            {
                continue;
            }
            // 删除目标必须是受控目录的直接子文件；上面的 file_type 已排除链接。
            if path.parent() != Some(directory.as_path()) {
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            tx.execute("UPDATE local_files SET cache_path=NULL WHERE record_id IN (SELECT record_id FROM file_records WHERE object_id=?1)", [&object]).map_err(db_error)?;
            total = total.saturating_sub(size);
            released += size;
        }
        tx.commit().map_err(db_error)?;
        Ok(released)
    }

    /// 忽略保留天数与容量目标，立即回收全部可回收受控副本；仍保护未完成归档内容。
    pub fn reclaim_cache_force(&mut self) -> Result<u64> {
        if !self.staging_dir.exists() {
            return Ok(0);
        }
        let directory = fs::canonicalize(&self.staging_dir)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let mut released = 0u64;
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || chatvault_metadata::validate_hash(&entry.file_name().to_string_lossy()).is_err()
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let object = format!("blake3:{name}");
            let protected: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM local_files l JOIN file_records r USING(record_id)
                 WHERE r.object_id=?1 AND (
                   NOT EXISTS(SELECT 1 FROM upload_tasks t WHERE t.record_id=l.record_id AND t.status='backed_up')
                   OR EXISTS(SELECT 1 FROM upload_tasks t WHERE t.record_id=l.record_id AND t.status!='backed_up')
                   OR NOT EXISTS(SELECT 1 FROM journal_events j JOIN sync_cursors c ON c.device_id=j.device_id AND c.epoch=j.epoch
                     WHERE j.event_type='file_record_added' AND json_extract(j.payload,'$.record_id')=l.record_id AND c.last_contiguous_seq>=j.seq)
                 ))", [&object], |r| r.get(0)).map_err(db_error)?;
            if protected {
                continue;
            }
            let path = entry.path();
            if path.parent() != Some(directory.as_path()) {
                continue;
            }
            let size = entry.metadata()?.len();
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            tx.execute("UPDATE local_files SET cache_path=NULL WHERE record_id IN (SELECT record_id FROM file_records WHERE object_id=?1)", [&object]).map_err(db_error)?;
            released = released.saturating_add(size);
        }
        tx.commit().map_err(db_error)?;
        Ok(released)
    }

    /// 判断对象是否允许释放受控缓存：本机引用均已 backed_up 且元数据已发布。
    fn object_cache_releasable(tx: &rusqlite::Transaction, object_id: &str) -> Result<bool> {
        let protected: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM local_files l JOIN file_records r USING(record_id)
             WHERE r.object_id=?1 AND (
               NOT EXISTS(SELECT 1 FROM upload_tasks t WHERE t.record_id=l.record_id AND t.status='backed_up')
               OR EXISTS(SELECT 1 FROM upload_tasks t WHERE t.record_id=l.record_id AND t.status!='backed_up')
               OR NOT EXISTS(SELECT 1 FROM journal_events j JOIN sync_cursors c ON c.device_id=j.device_id AND c.epoch=j.epoch
                 WHERE j.event_type='file_record_added' AND json_extract(j.payload,'$.record_id')=l.record_id AND c.last_contiguous_seq>=j.seq)
             ))",
            [object_id],
            |r| r.get(0),
        )
        .map_err(db_error)?;
        Ok(!protected)
    }

    /// 释放指定内容对象的受控缓存副本；返回 (ok, released_bytes)。
    /// 未完成归档时返回 false 且不删文件。
    pub fn release_object_cache(&mut self, object_id: &str) -> Result<(bool, u64)> {
        let staging_dir = self.staging_dir.clone();
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;
        if !Self::object_cache_releasable(&tx, object_id)? {
            tx.commit().map_err(db_error)?;
            return Ok((false, 0));
        }
        let hash: Option<String> = tx
            .query_row(
                "SELECT hash FROM file_objects WHERE object_id=?1",
                [object_id],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        let mut released = 0u64;
        if let Some(hash) = hash {
            let path = staging_dir.join(&hash);
            if let Ok(meta) = fs::symlink_metadata(&path) {
                if meta.file_type().is_file() && !meta.file_type().is_symlink() {
                    match fs::remove_file(&path) {
                        Ok(()) => released = meta.len(),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e.into()),
                    }
                }
            }
        }
        tx.execute(
            "UPDATE local_files SET cache_path=NULL WHERE record_id IN (SELECT record_id FROM file_records WHERE object_id=?1)",
            [object_id],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok((true, released))
    }

    /// 读取已配置采集源根目录（空路径忽略）。
    fn collect_source_roots(&self) -> Result<Vec<String>> {
        let raw = self
            .get_setting(COLLECT_SOURCES_KEY)?
            .unwrap_or_else(|| "[]".to_string());
        let sources: Vec<CollectSource> =
            serde_json::from_str(&raw).map_err(|e| ChatVaultError::Internal(e.to_string()))?;
        Ok(sources
            .into_iter()
            .map(|s| s.path)
            .filter(|p| !p.trim().is_empty())
            .collect())
    }

    /// 删除内容对象对应的本机原文件与缓存副本，并移除 local_files 映射。
    ///
    /// 职责: 在完成远端归档与元数据发布的前提下，清理本机原文件与受控副本。
    /// 返回 (deleted_originals, deleted_cache, released_bytes)。路径不存在视为成功清理。
    ///
    /// 关键约束:
    ///   - 未完成归档/未发布元数据时拒绝删除，避免唯一副本丢失
    ///   - 原路径必须落在已配置采集源根之下（有采集源配置时）
    ///   - 缓存路径必须落在受控 staging 目录之下
    ///   - 先全量校验再删除；删除失败时已删文件对应映射仍会提交，避免索引悬空
    pub fn delete_object_local_files(&mut self, object_id: &str) -> Result<(usize, usize, u64)> {
        let staging_dir = self.staging_dir.clone();
        let allowed_roots = self.collect_source_roots()?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;

        if !Self::object_cache_releasable(&tx, object_id)? {
            return Err(ChatVaultError::Internal(
                "尚未完成远端归档与元数据同步，拒绝删除本机原文件".into(),
            ));
        }

        let mut rows = {
            let mut stmt = tx
                .prepare(
                    "SELECT l.record_id, l.original_path, l.cache_path
                     FROM local_files l
                     JOIN file_records r ON r.record_id = l.record_id
                     WHERE r.object_id = ?1",
                )
                .map_err(db_error)?;
            let mapped = stmt
                .query_map([object_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(db_error)?;
            let mut list = Vec::new();
            for item in mapped {
                list.push(item.map_err(db_error)?);
            }
            list
        };

        // 路径白名单校验：在触碰磁盘前全部完成，任一越界即中止。
        let staging_str = staging_dir.to_string_lossy().to_string();
        for (_, original_path, cache_path) in &rows {
            let original = original_path.trim();
            if !original.is_empty() {
                if is_under_root_canonical(original, &staging_str) {
                    return Err(ChatVaultError::Internal(
                        "原文件路径落在受控缓存目录内，拒绝删除".into(),
                    ));
                }
                if !allowed_roots.is_empty()
                    && !allowed_roots
                        .iter()
                        .any(|root| is_under_root_canonical(original, root))
                {
                    return Err(ChatVaultError::Internal(format!(
                        "原文件路径不在已配置采集目录内，拒绝删除: {original}"
                    )));
                }
            }
            if let Some(cache) = cache_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if !Self::is_under_staging(cache, &staging_dir) {
                    return Err(ChatVaultError::Internal(format!(
                        "缓存路径不在受控目录内，拒绝删除: {cache}"
                    )));
                }
            }
        }

        let mut deleted_originals = 0usize;
        let mut deleted_cache = 0usize;
        let mut released = 0u64;
        let mut errors: Vec<String> = Vec::new();
        let mut cleaned_record_ids: Vec<String> = Vec::new();

        let hash: Option<String> = tx
            .query_row(
                "SELECT hash FROM file_objects WHERE object_id=?1",
                [object_id],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if let Some(hash) = hash {
            let path = staging_dir.join(&hash);
            if Self::is_under_staging(&path.to_string_lossy(), &staging_dir) {
                if let Ok(meta) = fs::symlink_metadata(&path) {
                    if meta.file_type().is_file() && !meta.file_type().is_symlink() {
                        match fs::remove_file(&path) {
                            Ok(()) => {
                                deleted_cache += 1;
                                released = released.saturating_add(meta.len());
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => {
                                errors.push(format!("删除缓存副本失败 {}: {e}", path.display()))
                            }
                        }
                    }
                }
            }
        }

        for (record_id, original_path, cache_path) in rows.drain(..) {
            let mut record_ok = true;
            let original = original_path.trim();
            if !original.is_empty() {
                match fs::symlink_metadata(original) {
                    Ok(meta) if meta.file_type().is_file() && !meta.file_type().is_symlink() => {
                        match fs::remove_file(original) {
                            Ok(()) => {
                                deleted_originals += 1;
                                released = released.saturating_add(meta.len());
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => {
                                record_ok = false;
                                errors.push(format!("删除原文件失败 {original}: {e}"));
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        record_ok = false;
                        errors.push(format!("读取原文件失败 {original}: {e}"));
                    }
                }
            }
            if let Some(cache) = cache_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if Self::is_under_staging(cache, &staging_dir) {
                    if let Ok(meta) = fs::symlink_metadata(cache) {
                        if meta.file_type().is_file() && !meta.file_type().is_symlink() {
                            match fs::remove_file(cache) {
                                Ok(()) => {
                                    deleted_cache += 1;
                                    released = released.saturating_add(meta.len());
                                }
                                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                                Err(e) => {
                                    // 缓存删除失败不阻止映射清理：对象已不再引用本机路径。
                                    tracing::warn!("删除映射缓存失败 {cache}: {e}");
                                }
                            }
                        }
                    }
                }
            }
            // 仅清理已成功处理（或本就不存在）的映射，失败记录保留以便重试。
            if record_ok {
                cleaned_record_ids.push(record_id);
            }
        }

        for record_id in &cleaned_record_ids {
            tx.execute("DELETE FROM local_files WHERE record_id=?1", [record_id])
                .map_err(db_error)?;
        }

        tx.commit().map_err(db_error)?;
        if !errors.is_empty() {
            return Err(ChatVaultError::Internal(format!(
                "部分本机文件删除失败: {}",
                errors.join("；")
            )));
        }
        Ok((deleted_originals, deleted_cache, released))
    }

    /// 判断路径是否为受控 staging 目录内的直接子文件（不进入 open/ 等子目录）。
    fn is_under_staging(path: &str, staging_dir: &Path) -> bool {
        let path_key = chatvault_core::canonical_path_key(path);
        let root_key = chatvault_core::canonical_path_key(&staging_dir.to_string_lossy());
        if root_key.is_empty() {
            return false;
        }
        let Some(rest) = path_key.strip_prefix(&format!("{root_key}\\")).or_else(|| {
            if path_key == root_key {
                Some("")
            } else {
                None
            }
        }) else {
            return false;
        };
        !rest.is_empty() && !rest.contains('\\')
    }
}

/// 将 SQLite 错误映射为领域错误。
fn db_error(e: rusqlite::Error) -> ChatVaultError {
    ChatVaultError::Database(e.to_string())
}
