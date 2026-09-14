//! # 文件检索与全文查询模块
//!
//! 提供基于 SQLite FTS5 的中文全文搜索、单字/双字模糊匹配回退，
//! 以及基于文件类型、账号、时间范围的多维组合过滤与分页查询。

use crate::db::Database;
use chatvault_core::error::{ChatVaultError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 检索过滤条件
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    /// 检索关键词 (支持中文、英文、数字子串)
    pub keyword: Option<String>,
    /// 文件扩展名过滤 (例如 "pdf", "docx")
    pub extension: Option<String>,
    /// 文件分类，在数据库分页前筛选。
    pub category: Option<String>,
    /// 来源类型过滤
    pub source_type: Option<String>,
    /// 来源账号过滤 (例如微信号或 wxid)
    pub source_account_id: Option<String>,
    /// 来源聊天过滤
    pub source_conversation_id: Option<String>,
    /// 时间范围起始
    pub start_time: Option<DateTime<Utc>>,
    /// 时间范围截止
    pub end_time: Option<DateTime<Utc>>,
    /// 返回的最大记录数
    pub limit: usize,
    /// 分页偏移量
    pub offset: usize,
}

/// 检索结果项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub record_id: String,
    pub object_id: String,
    pub original_name: String,
    pub extension: String,
    pub size: u64,
    pub source_type: String,
    pub source_account_id: Option<String>,
    pub source_account_name: Option<String>,
    pub source_conversation_id: Option<String>,
    pub source_conversation_name: Option<String>,
    pub file_time: String,
    pub original_path: Option<String>,
    pub upload_status: Option<String>,
    pub discovered_at: String,
}

/// 内容对象在本机/远端的位置状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectLocation {
    /// 本机可读，尚未确认远端归档
    Local,
    /// 本机无可用路径，内容在远端
    Remote,
    /// 本机可读且远端已归档
    Both,
    /// 仅有失效本地映射且无远端归档
    Missing,
}

impl ObjectLocation {
    /// 转为前端使用的短标识
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectLocation::Local => "local",
            ObjectLocation::Remote => "remote",
            ObjectLocation::Both => "both",
            ObjectLocation::Missing => "missing",
        }
    }
}

/// 按内容对象折叠后的列表项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectSearchItem {
    pub object_id: String,
    pub hash: String,
    pub original_name: String,
    pub extension: String,
    pub size: u64,
    pub file_time: String,
    pub discovered_at: String,
    pub source_count: usize,
    pub location: ObjectLocation,
    /// 本机可打开路径；解密来源只允许使用明文 cache_path。
    pub open_path: Option<String>,
}

/// 内容对象展开后的单条来源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectSourceItem {
    pub record_id: String,
    pub original_name: String,
    pub original_path: Option<String>,
    pub source_type: String,
    pub source_account_id: Option<String>,
    pub source_account_name: Option<String>,
    pub source_conversation_id: Option<String>,
    pub source_conversation_name: Option<String>,
    pub file_time: String,
    pub discovered_at: String,
    pub device_id: String,
    /// 远端设备注册名；本机设备可能尚未发布
    pub device_name: Option<String>,
    pub has_local_path: bool,
}

/// 检索服务
pub struct SearchService<'a> {
    db: &'a Database,
}

impl<'a> SearchService<'a> {
    /// 创建检索服务实例
    ///
    /// 职责: 绑定数据库引用初始化检索服务
    /// 输入: `db`: Database 引用
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 执行多维条件查询
    ///
    /// 职责: 根据关键词（FTS5 / LIKE 回退）与结构化条件执行分页查询
    /// 输入: `filter`: 过滤条件
    /// 输出: `Result<Vec<SearchResultItem>>`
    /// 关键约束:
    ///   - 当关键词字符数 >= 3 时，使用 FTS5 MATCH 查询加速
    ///   - 当关键词字符数 < 3（如单字、双字）时，使用 LIKE '%keyword%' 回退匹配
    pub fn search(&self, filter: &SearchFilter) -> Result<Vec<SearchResultItem>> {
        let conn = self.db.connection();
        let limit = if filter.limit == 0 {
            50
        } else {
            filter.limit.min(500)
        };
        let offset = filter.offset;

        let mut conditions: Vec<String> = vec![
            "NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id=r.record_id)".into(),
        ];
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // 1. 关键词查询处理
        if let Some(kw) = &filter.keyword {
            let trimmed = kw.trim();
            if !trimmed.is_empty() {
                let char_count = trimmed.chars().count();
                if char_count >= 3 {
                    // trigram FTS5 查询: 将查询词包装为双引号精确子串
                    conditions.push(
                        "r.record_id IN (SELECT record_id FROM file_search_fts WHERE file_search_fts MATCH ?)"
                            .to_string(),
                    );
                    params_vec.push(Box::new(format!("\"{}\"", trimmed.replace('"', "\"\""))));
                } else {
                    // 短词或单字，回退使用 LIKE 匹配
                    conditions.push("r.original_name LIKE ? ESCAPE '\\'".to_string());
                    let literal = trimmed
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_");
                    params_vec.push(Box::new(format!("%{}%", literal)));
                }
            }
        }

        if let Some(category) = &filter.category {
            if let Some(condition) = crate::category::category_condition(category)? {
                conditions.push(condition);
            }
        }

        // 2. 扩展名过滤
        if let Some(ext) = &filter.extension {
            let clean_ext = ext.trim_start_matches('.').to_lowercase();
            conditions.push("o.extension = ?".to_string());
            params_vec.push(Box::new(clean_ext));
        }

        // 3. 来源实体过滤
        if let Some(source_type) = &filter.source_type {
            conditions.push("r.source_type = ?".to_string());
            params_vec.push(Box::new(source_type.clone()));
        }
        if let Some(acc) = &filter.source_account_id {
            conditions.push("r.source_account_id = ?".to_string());
            params_vec.push(Box::new(acc.clone()));
        }
        if let Some(conversation_id) = &filter.source_conversation_id {
            conditions.push("r.source_conversation_id = ?".to_string());
            params_vec.push(Box::new(conversation_id.clone()));
        }

        // 4. 起始时间
        if let Some(st) = &filter.start_time {
            conditions.push("r.file_time >= ?".to_string());
            params_vec.push(Box::new(st.to_rfc3339()));
        }

        // 5. 截止时间
        if let Some(et) = &filter.end_time {
            conditions.push("r.file_time <= ?".to_string());
            params_vec.push(Box::new(et.to_rfc3339()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            r#"
            SELECT 
                r.record_id,
                r.object_id,
                r.original_name,
                o.extension,
                o.size,
                r.source_type,
                r.source_account_id,
                CASE WHEN r.source_account_id IS NULL THEN NULL
                     ELSE COALESCE(NULLIF(sa.display_name, ''), NULLIF(sa.source_name, ''), r.source_account_id)
                END AS source_account_name,
                r.source_conversation_id,
                CASE WHEN r.source_conversation_id IS NULL THEN NULL
                     ELSE COALESCE(NULLIF(sc.display_name, ''), NULLIF(sc.source_name, ''), r.source_conversation_id)
                END AS source_conversation_name,
                r.file_time,
                l.original_path,
                t.status AS upload_status,
                r.discovered_at
            FROM file_records r
            JOIN file_objects o ON r.object_id = o.object_id
            LEFT JOIN local_files l ON r.record_id = l.record_id
            LEFT JOIN upload_tasks t ON r.record_id = t.record_id
            LEFT JOIN source_accounts sa
              ON sa.source_type = r.source_type
             AND sa.source_account_id = r.source_account_id
            LEFT JOIN source_conversations sc
              ON sc.source_type = r.source_type
             AND sc.source_account_id = r.source_account_id
             AND sc.source_conversation_id = r.source_conversation_id
            {}
            ORDER BY r.file_time DESC, r.record_id
            LIMIT ? OFFSET ?
            "#,
            where_clause
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| ChatVaultError::Database(format!("准备查询 SQL 失败: {}", e)))?;

        // 构造动态参数列表
        let mut query_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
        for p in &params_vec {
            query_params.push(p.as_ref());
        }
        let limit_val = limit as i64;
        let offset_val = offset as i64;
        query_params.push(&limit_val);
        query_params.push(&offset_val);

        let rows = stmt
            .query_map(query_params.as_slice(), |row| {
                Ok(SearchResultItem {
                    record_id: row.get(0)?,
                    object_id: row.get(1)?,
                    original_name: row.get(2)?,
                    extension: row.get(3)?,
                    size: row.get::<_, i64>(4)? as u64,
                    source_type: row.get(5)?,
                    source_account_id: row.get(6)?,
                    source_account_name: row.get(7)?,
                    source_conversation_id: row.get(8)?,
                    source_conversation_name: row.get(9)?,
                    file_time: row.get(10)?,
                    original_path: row.get(11)?,
                    upload_status: row.get(12)?,
                    discovered_at: row.get(13)?,
                })
            })
            .map_err(|e| ChatVaultError::Database(format!("执行查询失败: {}", e)))?;

        let mut results = Vec::new();
        for item in rows {
            results.push(item.map_err(|e| ChatVaultError::Database(e.to_string()))?);
        }

        Ok(results)
    }

    /// 按内容对象折叠检索：同一 object_id 只返回代表行（file_time 最新）
    ///
    /// 职责: 在记录级过滤后按 object 去重分页，并推断本机/远端位置
    /// 输入: `filter`: 与记录检索相同的过滤条件；`device_id`: 本机设备标识
    /// 输出: `Result<Vec<ObjectSearchItem>>`
    pub fn search_objects(
        &self,
        filter: &SearchFilter,
        device_id: &str,
    ) -> Result<Vec<ObjectSearchItem>> {
        let conn = self.db.connection();
        let limit = if filter.limit == 0 {
            50
        } else {
            filter.limit.min(500)
        };
        let offset = filter.offset;
        let (where_clause, params_vec) = build_record_conditions(filter)?;

        let sql = format!(
            r#"
            WITH filtered AS (
                SELECT
                    r.record_id,
                    r.object_id,
                    r.original_name,
                    r.file_time,
                    r.discovered_at,
                    r.device_id,
                    o.hash,
                    o.extension,
                    o.size
                FROM file_records r
                JOIN file_objects o ON r.object_id = o.object_id
                {where_clause}
            ),
            ranked AS (
                SELECT
                    filtered.*,
                    ROW_NUMBER() OVER (
                        PARTITION BY object_id
                        ORDER BY file_time DESC, record_id
                    ) AS rn,
                    (
                        SELECT COUNT(*)
                        FROM file_records r2
                        WHERE r2.object_id = filtered.object_id
                          AND NOT EXISTS(
                              SELECT 1 FROM record_tombstones d WHERE d.record_id = r2.record_id
                          )
                    ) AS source_count
                FROM filtered
            )
            SELECT
                object_id,
                hash,
                original_name,
                extension,
                size,
                file_time,
                discovered_at,
                source_count
            FROM ranked
            WHERE rn = 1
            ORDER BY file_time DESC, object_id
            LIMIT ? OFFSET ?
            "#
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| ChatVaultError::Database(format!("准备对象检索 SQL 失败: {}", e)))?;

        let mut query_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
        for p in &params_vec {
            query_params.push(p.as_ref());
        }
        let limit_val = limit as i64;
        let offset_val = offset as i64;
        query_params.push(&limit_val);
        query_params.push(&offset_val);

        let rows = stmt
            .query_map(query_params.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)? as u64,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)? as usize,
                ))
            })
            .map_err(|e| ChatVaultError::Database(format!("执行对象检索失败: {}", e)))?;

        let mut base_items = Vec::new();
        for row in rows {
            base_items.push(row.map_err(|e| ChatVaultError::Database(e.to_string()))?);
        }

        let mut results = Vec::with_capacity(base_items.len());
        // 批量拉取位置信息，避免每条对象 N+1 查询
        let object_ids: Vec<String> = base_items.iter().map(|b| b.0.clone()).collect();
        let location_map = self.batch_object_locations(&object_ids, device_id)?;
        for (
            object_id,
            hash,
            original_name,
            extension,
            size,
            file_time,
            discovered_at,
            source_count,
        ) in base_items
        {
            let (open_path, location) = location_map
                .get(&object_id)
                .cloned()
                .unwrap_or((None, ObjectLocation::Missing));
            results.push(ObjectSearchItem {
                object_id,
                hash,
                original_name,
                extension,
                size,
                file_time,
                discovered_at,
                source_count,
                location,
                open_path,
            });
        }
        Ok(results)
    }

    /// 列出内容对象的全部有效来源记录
    pub fn list_object_sources(&self, object_id: &str) -> Result<Vec<ObjectSourceItem>> {
        let conn = self.db.connection();
        let sql = r#"
            SELECT
                r.record_id,
                r.original_name,
                r.source_type,
                r.source_account_id,
                CASE WHEN r.source_account_id IS NULL THEN NULL
                     ELSE COALESCE(NULLIF(sa.display_name, ''), NULLIF(sa.source_name, ''), r.source_account_id)
                END AS source_account_name,
                r.source_conversation_id,
                CASE WHEN r.source_conversation_id IS NULL THEN NULL
                     ELSE COALESCE(NULLIF(sc.display_name, ''), NULLIF(sc.source_name, ''), r.source_conversation_id)
                END AS source_conversation_name,
                r.file_time,
                r.discovered_at,
                r.device_id,
                NULLIF(kd.display_name, '') AS device_name,
                l.original_path,
                l.cache_path
            FROM file_records r
            LEFT JOIN local_files l ON r.record_id = l.record_id
            LEFT JOIN source_accounts sa
              ON sa.source_type = r.source_type
             AND sa.source_account_id = r.source_account_id
            LEFT JOIN source_conversations sc
              ON sc.source_type = r.source_type
             AND sc.source_account_id = r.source_account_id
             AND sc.source_conversation_id = r.source_conversation_id
            LEFT JOIN known_devices kd ON kd.device_id = r.device_id
            WHERE r.object_id = ?1
              AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
            ORDER BY r.file_time DESC, r.record_id
        "#;

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| ChatVaultError::Database(format!("准备来源展开 SQL 失败: {}", e)))?;
        let rows = stmt
            .query_map([object_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                ))
            })
            .map_err(|e| ChatVaultError::Database(format!("执行来源展开失败: {}", e)))?;

        let mut results = Vec::new();
        for row in rows {
            let (
                record_id,
                original_name,
                source_type,
                source_account_id,
                source_account_name,
                source_conversation_id,
                source_conversation_name,
                file_time,
                discovered_at,
                device_id,
                device_name,
                original_path,
                cache_path,
            ) = row.map_err(|e| ChatVaultError::Database(e.to_string()))?;

            let has_local_path =
                path_exists(original_path.as_deref()) || path_exists(cache_path.as_deref());
            results.push(ObjectSourceItem {
                record_id,
                original_name,
                original_path,
                source_type,
                source_account_id,
                source_account_name,
                source_conversation_id,
                source_conversation_name,
                file_time,
                discovered_at,
                device_id,
                device_name,
                has_local_path,
            });
        }
        Ok(results)
    }

    /// 批量推断多个内容对象的可打开路径与位置状态
    ///
    /// 用集合查询替代逐对象 N+1：本地路径、backed_up、外机记录。
    fn batch_object_locations(
        &self,
        object_ids: &[String],
        device_id: &str,
    ) -> Result<std::collections::HashMap<String, (Option<String>, ObjectLocation)>> {
        use std::collections::{HashMap, HashSet};

        let mut map = HashMap::new();
        if object_ids.is_empty() {
            return Ok(map);
        }
        let conn = self.db.connection();
        let placeholders = vec!["?"; object_ids.len()].join(",");
        let id_refs: Vec<&dyn rusqlite::ToSql> = object_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();

        // 1. 本地路径与可读性
        let mut any_local: HashSet<String> = HashSet::new();
        let mut local_readable: HashSet<String> = HashSet::new();
        let mut open_original: HashMap<String, String> = HashMap::new();
        let mut open_cache: HashMap<String, String> = HashMap::new();
        let sql = format!(
            r#"
            SELECT r.object_id, l.original_path, l.cache_path
            FROM local_files l
            JOIN file_records r ON r.record_id = l.record_id
            WHERE r.object_id IN ({placeholders})
              AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
            "#
        );
        {
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(id_refs.as_slice(), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            for row in rows {
                let (object_id, original_path, cache_path) =
                    row.map_err(|e| ChatVaultError::Database(e.to_string()))?;
                any_local.insert(object_id.clone());
                if path_exists(Some(&original_path)) {
                    local_readable.insert(object_id.clone());
                    open_original.entry(object_id).or_insert(original_path);
                } else if path_exists(cache_path.as_deref()) {
                    local_readable.insert(object_id.clone());
                    if let Some(cache) = cache_path {
                        open_cache.entry(object_id).or_insert(cache);
                    }
                }
            }
        }

        // 2. 远端已归档
        let mut backed_up: HashSet<String> = HashSet::new();
        let sql = format!(
            "SELECT DISTINCT object_id FROM upload_tasks WHERE object_id IN ({placeholders}) AND status = 'backed_up'"
        );
        {
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(id_refs.as_slice(), |row| row.get::<_, String>(0))
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            for row in rows {
                backed_up.insert(row.map_err(|e| ChatVaultError::Database(e.to_string()))?);
            }
        }

        // 3. 外机记录（无本地映射时推断远端）
        let mut foreign: HashSet<String> = HashSet::new();
        let sql = format!(
            r#"
            SELECT DISTINCT r.object_id
            FROM file_records r
            WHERE r.object_id IN ({placeholders})
              AND r.device_id <> ?{}
              AND NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id = r.record_id)
            "#,
            object_ids.len() + 1
        );
        {
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            let mut id_refs2 = id_refs.clone();
            id_refs2.push(&device_id as &dyn rusqlite::ToSql);
            let rows = stmt
                .query_map(id_refs2.as_slice(), |row| row.get::<_, String>(0))
                .map_err(|e| ChatVaultError::Database(e.to_string()))?;
            for row in rows {
                foreign.insert(row.map_err(|e| ChatVaultError::Database(e.to_string()))?);
            }
        }

        for id in object_ids {
            let has_local = local_readable.contains(id);
            let has_row = any_local.contains(id);
            let open_path = open_original
                .get(id)
                .or_else(|| open_cache.get(id))
                .cloned();
            let has_remote = backed_up.contains(id) || (foreign.contains(id) && !has_row);
            let location = match (has_local, has_remote, has_row) {
                (true, true, _) => ObjectLocation::Both,
                (true, false, _) => ObjectLocation::Local,
                (false, true, _) => ObjectLocation::Remote,
                // 无本地映射且无远端证据：视为失效，避免误标远程
                (false, false, _) => ObjectLocation::Missing,
            };
            map.insert(id.clone(), (open_path, location));
        }
        Ok(map)
    }
}

/// 路径非空且在磁盘上存在
fn path_exists(path: Option<&str>) -> bool {
    path.map(|p| {
        if p.trim().is_empty() {
            return false;
        }
        std::fs::symlink_metadata(p)
            .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
            .unwrap_or(false)
    })
    .unwrap_or(false)
}

/// 构建记录级过滤 WHERE 子句与参数（供记录检索与对象检索共用）
fn build_record_conditions(
    filter: &SearchFilter,
) -> Result<(String, Vec<Box<dyn rusqlite::ToSql>>)> {
    let mut conditions: Vec<String> =
        vec!["NOT EXISTS(SELECT 1 FROM record_tombstones d WHERE d.record_id=r.record_id)".into()];
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(kw) = &filter.keyword {
        let trimmed = kw.trim();
        if !trimmed.is_empty() {
            let char_count = trimmed.chars().count();
            if char_count >= 3 {
                conditions.push(
                    "r.record_id IN (SELECT record_id FROM file_search_fts WHERE file_search_fts MATCH ?)"
                        .to_string(),
                );
                params_vec.push(Box::new(format!("\"{}\"", trimmed.replace('"', "\"\""))));
            } else {
                conditions.push("r.original_name LIKE ? ESCAPE '\\'".to_string());
                let literal = trimmed
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                params_vec.push(Box::new(format!("%{}%", literal)));
            }
        }
    }

    if let Some(category) = &filter.category {
        if let Some(condition) = crate::category::category_condition(category)? {
            conditions.push(condition);
        }
    }

    if let Some(ext) = &filter.extension {
        let clean_ext = ext.trim_start_matches('.').to_lowercase();
        conditions.push("o.extension = ?".to_string());
        params_vec.push(Box::new(clean_ext));
    }

    if let Some(source_type) = &filter.source_type {
        conditions.push("r.source_type = ?".to_string());
        params_vec.push(Box::new(source_type.clone()));
    }
    if let Some(acc) = &filter.source_account_id {
        conditions.push("r.source_account_id = ?".to_string());
        params_vec.push(Box::new(acc.clone()));
    }
    if let Some(conversation_id) = &filter.source_conversation_id {
        conditions.push("r.source_conversation_id = ?".to_string());
        params_vec.push(Box::new(conversation_id.clone()));
    }
    if let Some(st) = &filter.start_time {
        conditions.push("r.file_time >= ?".to_string());
        params_vec.push(Box::new(st.to_rfc3339()));
    }
    if let Some(et) = &filter.end_time {
        conditions.push("r.file_time <= ?".to_string());
        params_vec.push(Box::new(et.to_rfc3339()));
    }

    let where_clause = format!("WHERE {}", conditions.join(" AND "));
    Ok((where_clause, params_vec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use chatvault_core::models::DiscoveredFile;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_chinese_search() {
        let mut db = Database::open_in_memory().unwrap();
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("AI超级个体-打造不可替代的个人品牌.pdf");

        let mut f = File::create(&file_path).unwrap();
        f.write_all(b"dummy pdf content").unwrap();
        drop(f);

        let df = DiscoveredFile {
            source_type: "wechat-test".to_string(),
            source_account_id: Some("wxid_test".to_string()),
            absolute_path: file_path.to_str().unwrap().to_string(),
            file_name: "AI超级个体-打造不可替代的个人品牌.pdf".to_string(),
            file_size: 17,
            modified_time: Utc::now(),
            source_conversation_id: None,
        };

        db.ingest_file(&df, "dev-test").unwrap();
        let service = SearchService::new(&db);

        // 1. 三字及以上 FTS5 检索测试
        let res1 = service
            .search(&SearchFilter {
                keyword: Some("超级个体".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res1.len(), 1);
        assert_eq!(
            res1[0].original_name,
            "AI超级个体-打造不可替代的个人品牌.pdf"
        );

        // 2. 双字 LIKE 回退测试
        let res2 = service
            .search(&SearchFilter {
                keyword: Some("品牌".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res2.len(), 1);

        // 3. 单字 LIKE 回退测试
        let res3 = service
            .search(&SearchFilter {
                keyword: Some("牌".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res3.len(), 1);

        // 4. 不存在的词
        let res4 = service
            .search(&SearchFilter {
                keyword: Some("毫无关系的内容".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res4.len(), 0);

        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn test_search_objects_collapses_duplicates() {
        let mut db = Database::open_in_memory().unwrap();
        let temp_dir = std::env::temp_dir();
        let shared = temp_dir.join(format!("chatvault-obj-shared-{}.txt", std::process::id()));
        let other = temp_dir.join(format!("chatvault-obj-other-{}.txt", std::process::id()));
        std::fs::write(&shared, b"shared-content").unwrap();
        std::fs::write(&other, b"other-content").unwrap();

        // 同一内容、两个来源
        db.ingest_file(
            &DiscoveredFile {
                source_type: "wechat-test".to_string(),
                source_account_id: Some("wxid_a".to_string()),
                absolute_path: shared.to_str().unwrap().to_string(),
                file_name: "共享附件.txt".to_string(),
                file_size: 14,
                modified_time: Utc::now(),
                source_conversation_id: None,
            },
            "dev-a",
        )
        .unwrap();
        // 换路径模拟第二条来源记录（同内容）
        let shared_copy = temp_dir.join(format!("chatvault-obj-copy-{}.txt", std::process::id()));
        std::fs::write(&shared_copy, b"shared-content").unwrap();
        db.ingest_file(
            &DiscoveredFile {
                source_type: "wechat-test".to_string(),
                source_account_id: Some("wxid_b".to_string()),
                absolute_path: shared_copy.to_str().unwrap().to_string(),
                file_name: "共享附件-副本.txt".to_string(),
                file_size: 14,
                modified_time: Utc::now(),
                source_conversation_id: None,
            },
            "dev-b",
        )
        .unwrap();
        db.ingest_file(
            &DiscoveredFile {
                source_type: "generic-folder".to_string(),
                source_account_id: None,
                absolute_path: other.to_str().unwrap().to_string(),
                file_name: "独立文件.txt".to_string(),
                file_size: 13,
                modified_time: Utc::now(),
                source_conversation_id: None,
            },
            "dev-a",
        )
        .unwrap();

        let service = SearchService::new(&db);
        let objects = service
            .search_objects(
                &SearchFilter {
                    limit: 50,
                    ..Default::default()
                },
                "dev-a",
            )
            .unwrap();

        // 两条内容对象（共享内容折叠为一行）
        assert_eq!(objects.len(), 2);
        let shared_obj = objects
            .iter()
            .find(|o| o.source_count == 2)
            .expect("应存在双来源对象");
        assert_eq!(shared_obj.location, ObjectLocation::Local);
        assert!(shared_obj
            .open_path
            .as_ref()
            .unwrap()
            .contains("chatvault-obj"));

        let sources = service.list_object_sources(&shared_obj.object_id).unwrap();
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|s| s.has_local_path));

        let _ = std::fs::remove_file(shared);
        let _ = std::fs::remove_file(shared_copy);
        let _ = std::fs::remove_file(other);
    }

    #[test]
    fn test_object_location_remote_from_other_device() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.connection();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO file_objects (object_id, hash, size, mime, extension, created_at)
             VALUES ('blake3:abc', 'abc', 10, 'text/plain', 'txt', ?1)",
            rusqlite::params![now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_records (record_id, object_id, source_type, source_account_id, source_conversation_id, original_name, file_time, time_source, discovered_at, device_id)
             VALUES ('r1', 'blake3:abc', 'wechat-test', NULL, NULL, '远端.txt', ?1, 'mtime', ?1, 'other-device')",
            rusqlite::params![now],
        )
        .unwrap();

        let service = SearchService::new(&db);
        let objects = service
            .search_objects(&SearchFilter::default(), "dev-local")
            .unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].location, ObjectLocation::Remote);
        assert!(objects[0].open_path.is_none());
    }
}
