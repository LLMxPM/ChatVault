// ChatVault PROPFIND 解析：按 DAV 命名空间识别资源，支持任意前缀与 XML 实体。
use chatvault_core::error::{ChatVaultError, Result};
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};

/// 提取集合的直接子资源，拒绝非法 XML；路径保留 URL 编码供后续请求使用。
pub(crate) fn parse_propfind_hrefs(xml: &str, collection_rel: &str) -> Result<Vec<String>> {
    let collection = collection_rel.trim_matches('/');
    let base = format!("/{collection}/");
    let mut reader = NsReader::from_str(xml);
    let mut paths = Vec::new();
    loop {
        match reader.read_resolved_event().map_err(xml_error)? {
            (ResolveResult::Bound(ns), Event::Start(start))
                if ns.as_ref() == "DAV:" && start.local_name().as_ref() == "href" =>
            {
                let raw = reader.read_text(start.name()).map_err(xml_error)?;
                let href = quick_xml::escape::unescape(&raw).map_err(xml_error)?;
                if let Some(index) = href.rfind(&base) {
                    let name = href[index + base.len()..].trim_end_matches('/');
                    if !name.is_empty() && !name.contains('/') {
                        paths.push(format!("{collection}/{name}"));
                    }
                }
            }
            (_, Event::Eof) => break,
            _ => {}
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// 转换 XML 错误为可向用户报告的协议错误。
fn xml_error(e: impl std::fmt::Display) -> ChatVaultError {
    ChatVaultError::WebDav(format!("PROPFIND XML 无效: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 各种合法命名空间写法与实体转义得到相同结果。
    #[test]
    fn namespace_variants() {
        for (prefix, declaration) in [("D:", "xmlns:D"), ("d:", "xmlns:d"), ("", "xmlns")] {
            let xml = format!("<{prefix}multistatus {declaration}=\"DAV:\"><{prefix}response><{prefix}href>/dav/ChatVault/v/devices/a&amp;b.json</{prefix}href></{prefix}response></{prefix}multistatus>");
            assert_eq!(
                parse_propfind_hrefs(&xml, "ChatVault/v/devices").unwrap(),
                vec!["ChatVault/v/devices/a&b.json"]
            );
        }
        assert!(parse_propfind_hrefs("<broken", "v").is_err());
    }
}
