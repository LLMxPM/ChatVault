//! # 路径规范化工具
//!
//! 统一扫描根、本机文件路径的比对键，避免 Windows 分隔符、扩展前缀与大小写差异导致漏匹配。

/// 去掉 Windows 扩展路径前缀并统一分隔符与大小写，生成稳定比对键
///
/// 职责: 将任意形式的本地路径规范为可哈希比较的键
/// 输入: 原始路径字符串
/// 输出: 小写、反斜杠分隔、无尾部分隔符、无 `\\?\` 前缀的键
pub fn normalize_scan_key(path: &str) -> String {
    let mut p = path.replace('/', "\\");
    if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
        p = format!(r"\\{rest}");
    } else if let Some(rest) = p.strip_prefix(r"\\?\") {
        p = rest.to_string();
    }
    let trimmed = p.trim_end_matches('\\');
    trimmed.to_ascii_lowercase()
}

/// 判断文件路径是否位于扫描根之下（含根本身）
pub fn is_under_root(path: &str, root: &str) -> bool {
    let file_key = normalize_scan_key(path);
    let root_key = normalize_scan_key(root);
    if root_key.is_empty() {
        return false;
    }
    file_key == root_key || file_key.starts_with(&format!("{root_key}\\"))
}

/// 规范化扫描根：去掉尾部分隔符，便于持久化与前缀比较
pub fn normalize_root_path(path: &str) -> String {
    normalize_scan_key(path)
}

/// 去掉 Windows 扩展路径前缀（`\\?\` / `\\?\UNC\`），返回可与入库路径比对的形式
pub fn strip_extended_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// 将路径规范为可比对键：尽量 canonicalize 后去掉扩展前缀
///
/// 路径不存在时退回「父目录 canonicalize + 文件名拼接」，仍失败则原样返回，
/// 以兼容 Windows 短路径（如 `RUNNER~1`）与长路径混用的场景。
pub fn canonical_path_key(path: &str) -> String {
    let p = std::path::Path::new(path);
    if let Ok(canonical) = std::fs::canonicalize(p) {
        return strip_extended_prefix(&canonical.to_string_lossy());
    }
    if let (Some(parent), Some(name)) = (p.parent(), p.file_name()) {
        if !parent.as_os_str().is_empty() {
            if let Ok(parent_canonical) = std::fs::canonicalize(parent) {
                let joined = parent_canonical.join(name);
                return strip_extended_prefix(&joined.to_string_lossy());
            }
        }
    }
    path.to_string()
}

/// 判断 path 是否位于 root 之下；两侧均先 canonicalize，兼容短路径
pub fn is_under_root_canonical(path: &str, root: &str) -> bool {
    is_under_root(&canonical_path_key(path), &canonical_path_key(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_extended_prefix() {
        assert_eq!(normalize_scan_key(r"\\?\C:\Foo\Bar.txt"), r"c:\foo\bar.txt");
        assert_eq!(
            normalize_scan_key(r"\\?\UNC\server\share\a"),
            r"\\server\share\a"
        );
        assert_eq!(normalize_scan_key(r"C:/Foo/Bar/"), r"c:\foo\bar");
    }

    #[test]
    fn test_under_root() {
        assert!(is_under_root(r"C:\a\b\c.txt", r"C:\a\b"));
        assert!(is_under_root(r"C:\a\b", r"C:\a\b"));
        assert!(!is_under_root(r"C:\a\bc.txt", r"C:\a\b"));
        assert!(!is_under_root(r"D:\a\b\c.txt", r"C:\a\b"));
    }

    #[test]
    fn test_strip_extended_prefix() {
        assert_eq!(
            strip_extended_prefix(r"\\?\C:\Foo\Bar.txt"),
            r"C:\Foo\Bar.txt"
        );
        assert_eq!(
            strip_extended_prefix(r"\\?\UNC\server\share\a"),
            r"\\server\share\a"
        );
        assert_eq!(strip_extended_prefix(r"C:\Foo\Bar"), r"C:\Foo\Bar");
    }

    #[test]
    fn test_canonical_path_key_matches_short_and_long() {
        let dir = std::env::temp_dir().join(format!("cv_path_key_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.txt");
        std::fs::write(&file, b"x").unwrap();
        let raw = dir.to_string_lossy().to_string();
        let canonical = std::fs::canonicalize(&dir).unwrap();
        let long = strip_extended_prefix(&canonical.to_string_lossy());
        assert!(is_under_root_canonical(&file.to_string_lossy(), &raw));
        assert!(is_under_root_canonical(
            &file.join("..").join("a.txt").to_string_lossy(),
            &long
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
