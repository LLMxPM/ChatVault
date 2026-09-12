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
}
