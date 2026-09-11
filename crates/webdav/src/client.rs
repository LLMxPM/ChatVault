//! # WebDAV 客户端核心实现
//!
//! 基于 reqwest 实现 RFC4918 核心动词（MKCOL, PUT, GET, HEAD, MOVE, DELETE），
//! 支持 Basic 认证、HTTPS 证书校验与目录层级递归创建。

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
    config: WebDavConfig,
    client: Client,
}

impl WebDavClient {
    /// 创建新的 WebDAV 客户端实例
    ///
    /// 职责: 构建 HTTP 客户端，配置超时与连接池
    /// 输入: `config`: 连接配置
    /// 输出: `Result<Self>`
    pub fn new(config: WebDavConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| ChatVaultError::WebDav(format!("创建 HTTP 客户端失败: {}", e)))?;

        Ok(Self { config, client })
    }

    /// 拼接完整的远端资源 URL
    ///
    /// 职责: 将相对路径安全拼接到 base_url 之后
    /// 输入: `relative_path`: 例如 `ChatVault/vault-1/config/vault.json`
    /// 输出: 完整 URL 字符串
    pub fn get_full_url(&self, relative_path: &str) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        let rel = relative_path.trim_start_matches('/');
        format!("{}/{}", base, rel)
    }

    /// 发起带认证的 HTTP 请求辅助方法
    fn build_request(&self, method: Method, full_url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.request(method, full_url);
        if let (Some(u), Some(p)) = (&self.config.username, &self.config.password) {
            req = req.basic_auth(u, Some(p));
        }
        req
    }

    /// HEAD 检查资源是否存在
    ///
    /// 职责: 快速探测远端文件或目录是否存在
    /// 输入: `relative_path`: 远端相对路径
    /// 输出: 存在返回 Ok(true)，不存在（404）返回 Ok(false)，其它返回 Err
    pub async fn exists(&self, relative_path: &str) -> Result<bool> {
        let url = self.get_full_url(relative_path);
        let resp = self
            .build_request(Method::HEAD, &url)
            .send()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("HEAD 请求异常: {}", e)))?;

        if resp.status().is_success() {
            Ok(true)
        } else if resp.status() == StatusCode::NOT_FOUND {
            Ok(false)
        } else {
            Err(ChatVaultError::WebDav(format!(
                "HEAD 响应异常状态码: {}",
                resp.status()
            )))
        }
    }

    /// MKCOL 创建单个集合目录
    ///
    /// 职责: 发送 MKCOL 动词创建目录
    /// 输入: `relative_path`: 目录相对路径
    /// 输出: 成功或已存在返回 Ok(())，失败返回 Err
    pub async fn mkcol(&self, relative_path: &str) -> Result<()> {
        let url = self.get_full_url(relative_path);
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
    /// 输出: `Result<()>`
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
    /// 输出: `Result<()>`
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

        let url = self.get_full_url(remote_path);
        let resp = self
            .build_request(Method::PUT, &url)
            .header("Content-Length", file_len.to_string())
            .body(body)
            .send()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("PUT 上传失败: {}", e)))?;

        let status = resp.status();
        if status == StatusCode::CREATED
            || status == StatusCode::NO_CONTENT
            || status.is_success()
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
    /// 输出: `Result<()>`
    pub async fn upload_bytes(&self, bytes: Vec<u8>, remote_path: &str) -> Result<()> {
        if let Some(parent) = Path::new(remote_path).parent() {
            let parent_str = parent.to_string_lossy().replace('\\', "/");
            if !parent_str.is_empty() {
                self.ensure_dir(&parent_str).await?;
            }
        }

        let url = self.get_full_url(remote_path);
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
    /// 输出: `Result<()>`
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

        let src_url = self.get_full_url(src_rel_path);
        let dest_url = self.get_full_url(dest_rel_path);

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
        if status == StatusCode::CREATED
            || status == StatusCode::NO_CONTENT
            || status.is_success()
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
        let url = self.get_full_url(relative_path);
        let resp = self
            .build_request(Method::GET, &url)
            .send()
            .await
            .map_err(|e| ChatVaultError::WebDav(format!("GET 发起失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(ChatVaultError::WebDav(format!(
                "GET 请求 {} 失败: 状态码 {}",
                relative_path,
                resp.status()
            )));
        }

        Ok(resp)
    }
}
