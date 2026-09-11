// ChatVault 文件分类：统一扩展名规则与分页前的数据库过滤条件。
use chatvault_core::error::{ChatVaultError, Result};

const CATEGORIES: &[(&str, &[&str])] = &[
    (
        "doc",
        &[
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "csv",
        ],
    ),
    (
        "image",
        &["jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "ico"],
    ),
    ("video", &["mp4", "mov", "avi", "mkv", "flv", "wmv"]),
    ("audio", &["mp3", "wav", "ogg", "flac", "m4a", "aac"]),
    ("archive", &["zip", "rar", "7z", "tar", "gz"]),
];

/// 按来源文件名分类，避免同内容不同扩展名被首个对象的扩展名误分类。
pub fn determine_category(name: &str) -> String {
    let lower = name.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    CATEGORIES
        .iter()
        .find(|(_, exts)| exts.contains(&ext))
        .map(|(name, _)| *name)
        .unwrap_or("other")
        .into()
}

/// 从固定白名单生成 SQL 条件，用户输入不直接拼接到 SQL。
pub(crate) fn category_condition(category: &str) -> Result<Option<String>> {
    if category == "all" || category.is_empty() {
        return Ok(None);
    }
    let exts: Vec<&str> = if category == "other" {
        CATEGORIES
            .iter()
            .flat_map(|(_, exts)| exts.iter().copied())
            .collect()
    } else {
        CATEGORIES
            .iter()
            .find(|(name, _)| *name == category)
            .ok_or_else(|| ChatVaultError::Internal("未知文件分类".into()))?
            .1
            .to_vec()
    };
    let tests = exts
        .iter()
        .map(|ext| format!("LOWER(r.original_name) LIKE '%.{ext}'"))
        .collect::<Vec<_>>()
        .join(" OR ");
    Ok(Some(format!(
        "{}({tests})",
        if category == "other" { "NOT " } else { "" }
    )))
}
