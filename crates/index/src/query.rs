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
    /// 账号过滤 (例如微信号或 wxid)
    pub account_id: Option<String>,
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
    pub account_id: Option<String>,
    pub conversation_id: Option<String>,
    pub file_time: String,
    pub original_path: Option<String>,
    pub upload_status: Option<String>,
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
        let limit = if filter.limit == 0 { 50 } else { filter.limit };
        let offset = filter.offset;

        let mut conditions: Vec<String> = Vec::new();
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
                    params_vec.push(Box::new(format!("\"{}\"", trimmed)));
                } else {
                    // 短词或单字，回退使用 LIKE 匹配
                    conditions.push("r.original_name LIKE ?".to_string());
                    params_vec.push(Box::new(format!("%{}%", trimmed)));
                }
            }
        }

        // 2. 扩展名过滤
        if let Some(ext) = &filter.extension {
            let clean_ext = ext.trim_start_matches('.').to_lowercase();
            conditions.push("o.extension = ?".to_string());
            params_vec.push(Box::new(clean_ext));
        }

        // 3. 账号过滤
        if let Some(acc) = &filter.account_id {
            conditions.push("r.account_id = ?".to_string());
            params_vec.push(Box::new(acc.clone()));
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
                r.account_id,
                r.conversation_id,
                r.file_time,
                l.original_path,
                t.status AS upload_status
            FROM file_records r
            JOIN file_objects o ON r.object_id = o.object_id
            LEFT JOIN local_files l ON r.record_id = l.record_id
            LEFT JOIN upload_tasks t ON r.record_id = t.record_id
            {}
            ORDER BY r.file_time DESC
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
                    account_id: row.get(5)?,
                    conversation_id: row.get(6)?,
                    file_time: row.get(7)?,
                    original_path: row.get(8)?,
                    upload_status: row.get(9)?,
                })
            })
            .map_err(|e| ChatVaultError::Database(format!("执行查询失败: {}", e)))?;

        let mut results = Vec::new();
        for item in rows {
            results.push(item.map_err(|e| ChatVaultError::Database(e.to_string()))?);
        }

        Ok(results)
    }
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
            account_id: Some("wxid_test".to_string()),
            absolute_path: file_path.to_str().unwrap().to_string(),
            file_name: "AI超级个体-打造不可替代的个人品牌.pdf".to_string(),
            file_size: 17,
            modified_time: Utc::now(),
            conversation_hint: None,
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
        assert_eq!(res1[0].original_name, "AI超级个体-打造不可替代的个人品牌.pdf");

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
}
