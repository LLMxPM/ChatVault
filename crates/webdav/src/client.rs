//! # WebDAV 客户端核心实现
//!
//! 基于 reqwest 实现 RFC4918 核心动词（MKCOL, PUT, GET, HEAD, MOVE, DELETE），
//! 支持 Basic 认证、HTTPS 证书校验与目录层级递归创建。

use crate::propfind::parse_propfind_hrefs;
use chatvault_core::error::{ChatVaultError, Result};
use reqwest::{Client, Method, Response, StatusCode};
use std::path::Path;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

/// WebDAV 服务器认证配置
#[derive(Debug, Clone)]
pub struct WebDavConfig {
    /// WebDAV 基础服务 URL，末尾可含或不含斜杠，例如 `https://dav.example.com`
    pub base_url: String,
    /// 访问账号
    pub username: Option<String>,
    /// 访问密码或应用密钥
    pub password: Option<String>,
}

/// WebDAV 客户端
pub struct WebDavClient {
    pub(crate) config: WebDavConfig,
    pub(crate) client: Client,
}

impl WebDavClient {
    /// 创建新的 WebDAV 客户端实例
    ///
    /// 职责: 构建 HTTP 客户端，配置超时与连接池
    /// 输入: `config`: 连接配置
    /// 输出: `Result<Self>`
    pub fn new(mut config: WebDavConfig) -> Result<Self> {
        let base = reqwest::Url::parse(config.base_url.trim())
            .map_err(|e| ChatVaultError::WebDav(format!("无效服务器地址: {e}")))?;
        let loopback = matches!(base.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if (base.scheme() != "https" && !(base.scheme() == "http" && loopback))
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(ChatVaultError::WebDav(
                "远端必须使用 HTTPS，地址不能包含凭据、查询或片段；HTTP 仅限本机测试".into(),
            ));
        }
        config.base_url = base.to_string().trim_end_matches('/').to_string();
        // 写操作（PUT/MOVE/DELETE/MKCOL/PROPFIND）不自动跟随重定向；
        // GET/HEAD 在 send_read 中手动跟随（兼容 123pan 等跨主机下载跳转）。
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(3600))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ChatVaultError::WebDav(format!("创建 HTTP 客户端失败: {}", e)))?;

        Ok(Self { config, client })
    }

    /// 拼接完整的远端资源 URL
    ///
    /// 职责: 将相对路径安全拼接到 base_url 之后
    /// 输入: `relative_path`: 例如 `chatvault-home/config/vault.json`
    /// 输出: 完整 URL 字符串
    pub fn get_full_url(&self, relative_path: &str) -> Result<String> {
        let base = self.config.base_url.trim_end_matches('/');
        let rel = relative_path.trim_start_matches('/');
        if rel.contains(['\\', '?', '#', '%', ':']) || rel.split('/').any(|p| p == "." || p == "..")
        {
            return Err(ChatVaultError::WebDav("远端相对路径不合法".into()));
        }
        Ok(format!("{}/{}", base, rel))
    }

    /// 发起带认证的 HTTP 请求辅助方法
    pub(crate) fn build_request(&self, method: Method, full_url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.request(method, full_url);
        if let (Some(u), Some(p)) = (&self.config.username, &self.config.password) {
            req = req.basic_auth(u, Some(p));
        }
        req
    }

    /// GET/HEAD 专用：有限次跟随 HTTPS 重定向。
    ///
    /// 123pan 等网关在 GET 时会 302 到下载 CDN（跨主机）。跨主机后剥离 Basic 凭据，
    /// 写路径仍使用 build_request 且不跟随跳转。
    async fn send_read(&self, method: Method, start_url: &str) -> Result<Response> {
        const MAX_HOPS: usize = 5;
        let mut url = start_url.to_string();
        let mut chain = vec![url.clone()];
        // 仅在与起始主机一致时携带凭据；跨主机跳转后清空。
        let start = reqwest::Url::parse(&url)
            .map_err(|e| ChatVaultError::WebDav(format!("解析 URL 失败: {e}")))?;
        let start_host = start.host_str().map(|s| s.to_string());
        let start_port = start.port_or_known_default();

        for _ in 0..=MAX_HOPS {
            let current = reqwest::Url::parse(&url)
                .map_err(|e| ChatVaultError::WebDav(format!("解析重定向 URL 失败: {e}")))?;
            let same_origin = current.host_str().map(|s| s.to_string()) == start_host
                && current.port_or_known_default() == start_port;

            let mut req = self.client.request(method.clone(), &url);
            if same_origin {
                if let (Some(u), Some(p)) = (&self.config.username, &self.config.password) {
                    req = req.basic_auth(u, Some(p));
                }
            }
            let resp = req
                .send()
                .await
                .map_err(|e| ChatVaultError::WebDav(format!("{method} 请求异常: {e}")))?;

            let status = resp.status();
            if status.is_success() || status == StatusCode::NOT_FOUND {
                return Ok(resp);
            }
            if !status.is_redirection() {
                return Err(ChatVaultError::WebDav(format!(
                    "{method} 请求 {} 失败: 状态码 {}",
                    chain.last().unwrap_or(&url),
                    status
                )));
            }

            let loc = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    ChatVaultError::WebDav(format!("{method} 收到 {status} 但缺少 Location 头"))
                })?;
            let next = current
                .join(loc)
                .map_err(|e| ChatVaultError::WebDav(format!("解析 Location 失败: {loc} ({e})")))?;
            if next.scheme() != "https"
                && !(next.scheme() == "http"
                    && matches!(next.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
            {
                return Err(ChatVaultError::WebDav(format!(
                    "拒绝非 HTTPS 重定向: {} -> {next}",
                    chain.last().unwrap_or(&url)
                )));
            }
            url = next.to_string();
            chain.push(url.clone());
        }

        Err(ChatVaultError::WebDav(format!(
            "{method} 重定向次数过多: {}",
            chain.join(" -> ")
        )))
    }

    /// HEAD 检查资源是否存在
    ///
    /// 职责: 快速探测远端文件或目录是否存在
    /// 输入: `relative_path`: 远端相对路径
    /// 输出: 存在返回 Ok(true)，不存在（404）返回 Ok(false)，其它返回 Err
    pub async fn exists(&self, relative_path: &str) -> Result<bool> {
        let url = self.get_full_url(relative_path)?;
        let resp = self.send_read(Method::HEAD, &url).await?;

        let status = resp.status();
        if status.is_success() {
            Ok(true)
        } else if status == StatusCode::NOT_FOUND {
            Ok(false)
        } else {
            Err(ChatVaultError::WebDav(format!(
                "HEAD 响应异常状态码: {}",
                status
            )))
        }
    }

    /// MKCOL 创建单个集合目录
    ///
    /// 职责: 发送 MKCOL 动词创建目录
    /// 输入: `relative_path`: 目录相对路径
    /// 输出: 成功或已存在返回 Ok(())，失败返回 Err
    pub async fn mkcol(&self, relative_path: &str) -> Result<()> {
        let url = self.get_full_url(relative_path)?;
        let resp = self
            .build_request(Method::from_bytes(b"MKCOL").unwrap(), &url)
            .send()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("MKCOL 发送失败: {}", e)))?;

        let status = resp.status();
        // 201 Created 表示创建成功；405 Method Not Allowed 通常表示目录已存在
        if status == StatusCode::CREATED
            || status == StatusCode::METHOD_NOT_ALLOWED
            || status.is_success()
        {
            Ok(())
        } else {
            Err(ChatVaultError::WebDav(format!(
                "创建集合目录失败 {}: 状态码 {}",
                relative_path, status
            )))
        }
    }

    /// 递归确保远端父级目录结构存在
    ///
    /// 职责: 将路径逐层拆解，并从顶级逐层执行 MKCOL
    /// 输入: `relative_path`: 目标文件或目录路径
    ///     输出: `Result<()>`
    pub async fn ensure_dir(&self, relative_path: &str) -> Result<()> {
        let parts: Vec<&str> = relative_path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        if parts.is_empty() {
            return Ok(());
        }

        let mut current = String::new();
        for part in parts {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            self.mkcol(&current).await?;
        }

        Ok(())
    }

    /// 上传本地文件到 WebDAV 远端指定路径
    ///
    /// 职责: 以流式读取方式通过 PUT 上传本地文件，自动确保父目录已创建
    /// 输入:
    ///   - `local_path`: 本地文件绝对路径
    ///   - `remote_path`: 远端相对路径
    ///     输出: `Result<()>`
    pub async fn upload_file<P: AsRef<Path>>(
        &self,
        local_path: P,
        remote_path: &str,
    ) -> Result<()> {
        let lp = local_path.as_ref();
        if !lp.exists() {
            return Err(ChatVaultError::FileNotFound {
                path: lp.display().to_string(),
            });
        }

        // 确保上级目录存在
        if let Some(parent) = Path::new(remote_path).parent() {
            let parent_str = parent.to_string_lossy().replace('\\', "/");
            if !parent_str.is_empty() {
                self.ensure_dir(&parent_str).await?;
            }
        }

        let file_len = lp.metadata()?.len();
        let file = File::open(lp).await?;
        let stream = ReaderStream::new(file);
        let body = reqwest::Body::wrap_stream(stream);

        let url = self.get_full_url(remote_path)?;
        let resp = self
            .build_request(Method::PUT, &url)
            .header("Content-Length", file_len.to_string())
            .body(body)
            .send()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("PUT 上传失败: {}", e)))?;

        let status = resp.status();
        if status == StatusCode::CREATED || status == StatusCode::NO_CONTENT || status.is_success()
        {
            Ok(())
        } else {
            Err(ChatVaultError::WebDav(format!(
                "上传文件至 {} 失败: 状态码 {}",
                remote_path, status
            )))
        }
    }

    /// 上传内存字节切片到远端
    ///
    /// 职责: 上传 JSON 配置、日志小片段等
    /// 输入:
    ///   - `bytes`: 数据切片
    ///   - `remote_path`: 目标相对路径
    ///     输出: `Result<()>`
    pub async fn upload_bytes(&self, bytes: Vec<u8>, remote_path: &str) -> Result<()> {
        if let Some(parent) = Path::new(remote_path).parent() {
            let parent_str = parent.to_string_lossy().replace('\\', "/");
            if !parent_str.is_empty() {
                self.ensure_dir(&parent_str).await?;
            }
        }

        let url = self.get_full_url(remote_path)?;
        let resp = self
            .build_request(Method::PUT, &url)
            .body(bytes)
            .send()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("PUT 字节失败: {}", e)))?;

        if resp.status().is_success() || resp.status() == StatusCode::CREATED {
            Ok(())
        } else {
            Err(ChatVaultError::WebDav(format!(
                "上传字节至 {} 失败: 状态码 {}",
                remote_path,
                resp.status()
            )))
        }
    }

    /// MOVE 移动或重命名远端资源
    ///
    /// 职责: 在 WebDAV 服务器端将文件从源相对路径移动到目标相对路径（常用于 staging -> objects）
    /// 输入:
    ///   - `src_rel_path`: 源相对路径
    ///   - `dest_rel_path`: 目标相对路径
    ///   - `overwrite`: 是否覆盖目标
    ///     输出: `Result<()>`
    pub async fn move_resource(
        &self,
        src_rel_path: &str,
        dest_rel_path: &str,
        overwrite: bool,
    ) -> Result<()> {
        // 确保目标父目录存在
        if let Some(parent) = Path::new(dest_rel_path).parent() {
            let parent_str = parent.to_string_lossy().replace('\\', "/");
            if !parent_str.is_empty() {
                self.ensure_dir(&parent_str).await?;
            }
        }

        let src_url = self.get_full_url(src_rel_path)?;
        let dest_url = self.get_full_url(dest_rel_path)?;

        let mut req = self
            .build_request(Method::from_bytes(b"MOVE").unwrap(), &src_url)
            .header("Destination", dest_url);

        if overwrite {
            req = req.header("Overwrite", "T");
        } else {
            req = req.header("Overwrite", "F");
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("MOVE 操作失败: {}", e)))?;

        let status = resp.status();
        if status == StatusCode::CREATED || status == StatusCode::NO_CONTENT || status.is_success()
        {
            Ok(())
        } else {
            Err(ChatVaultError::WebDav(format!(
                "移动资源从 {} 到 {} 失败: 状态码 {}",
                src_rel_path, dest_rel_path, status
            )))
        }
    }

    /// 发送 GET 请求获取响应流
    ///
    /// 职责: 提供远端文件流式下载句柄
    /// 输入: `relative_path`: 远端相对路径
    /// 输出: `Result<Response>`
    pub async fn get_stream(&self, relative_path: &str) -> Result<Response> {
        let url = self.get_full_url(relative_path)?;
        self.send_read(Method::GET, &url).await
    }

    /// 下载远端对象到本地路径，并流式校验 BLAKE3 哈希
    ///
    /// 职责: GET 对象流写入本地临时文件，校验通过后原子替换到 `dest_path`
    /// 输入:
    ///   - `remote_path`: 远端相对路径
    ///   - `dest_path`: 最终本地绝对路径（父目录需可写）
    ///   - `expected_hex`: 预期十六进制哈希（可含 `blake3:` 前缀）
    ///     输出: `Result<u64>` 写入字节数
    ///     关键约束:
    ///   - 校验失败删除临时文件，不覆盖目标
    ///   - 目标已存在时覆盖为校验通过的新文件
    pub async fn download_to_path<P: AsRef<Path>>(
        &self,
        remote_path: &str,
        dest_path: P,
        expected_hex: &str,
    ) -> Result<u64> {
        use futures_util::StreamExt;

        let dest = dest_path.as_ref();
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let clean_expected = expected_hex.trim_start_matches("blake3:");
        let resp = self.get_stream(remote_path).await?;
        if !resp.status().is_success() {
            return Err(ChatVaultError::WebDav(format!(
                "下载对象 {} 失败: 状态码 {}",
                remote_path,
                resp.status()
            )));
        }

        let tmp_path = dest.with_file_name(format!(
            "{}.part",
            dest.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "download".into())
        ));

        let mut stream = resp.bytes_stream();
        let mut file = tokio::fs::File::create(&tmp_path).await?;
        let mut hasher = blake3::Hasher::new();
        let mut total_bytes = 0u64;

        let write_result: Result<u64> = async {
            use tokio::io::AsyncWriteExt;
            while let Some(chunk_res) = stream.next().await {
                let chunk =
                    chunk_res.map_err(|e| ChatVaultError::WebDav(format!("下载流中断: {}", e)))?;
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
                total_bytes += chunk.len() as u64;
            }
            file.flush().await?;
            drop(file);

            let actual_hex = hasher.finalize().to_hex().to_string();
            if !actual_hex.eq_ignore_ascii_case(clean_expected) {
                return Err(ChatVaultError::HashMismatch {
                    expected: clean_expected.to_string(),
                    actual: actual_hex,
                });
            }

            tokio::fs::rename(&tmp_path, dest).await?;
            Ok(total_bytes)
        }
        .await;

        if write_result.is_err() {
            let _ = tokio::fs::remove_file(&tmp_path).await;
        }
        write_result
    }

    /// DELETE 删除远端资源
    ///
    /// 职责: 发送 DELETE 动词清理远端资源（用于异常清理或清理暂存）
    /// 输入: `relative_path`: 远端相对路径
    /// 输出: `Result<()>`
    pub async fn delete_resource(&self, relative_path: &str) -> Result<()> {
        let url = self.get_full_url(relative_path)?;
        let resp = self
            .build_request(Method::DELETE, &url)
            .send()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("DELETE 发起失败: {}", e)))?;

        let status = resp.status();
        if status.is_success()
            || status == StatusCode::NO_CONTENT
            || status == StatusCode::NOT_FOUND
        {
            Ok(())
        } else {
            Err(ChatVaultError::WebDav(format!(
                "DELETE 删除 {} 失败: 状态码 {}",
                relative_path, status
            )))
        }
    }

    /// PROPFIND Depth=1 列出集合下直接子资源的相对路径
    ///
    /// 职责: 发现设备注册、commit 等目录内容
    /// 输入: `relative_path`: 集合相对路径
    /// 输出: 子资源相对路径列表（不含目录自身）
    pub async fn list_dir(&self, relative_path: &str) -> Result<Vec<String>> {
        let url = self.get_full_url(relative_path)?;
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/><D:displayname/></D:prop></D:propfind>"#;

        let resp = self
            .build_request(Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("PROPFIND 发送失败: {}", e)))?;

        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !(status.is_success() || status.as_u16() == 207) {
            return Err(ChatVaultError::WebDav(format!(
                "PROPFIND {} 失败: 状态码 {}",
                relative_path, status
            )));
        }

        let text = resp
            .text()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("读取 PROPFIND 响应失败: {}", e)))?;

        parse_propfind_hrefs(&text, relative_path)
    }
}
