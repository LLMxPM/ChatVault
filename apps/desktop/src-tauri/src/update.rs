// 拾文应用更新：检查 GitHub 正式 Release 并下载安装 NSIS 安装包。
// 仅使用 /releases/latest，不会纳入预发布（alpha/rc 等）版本。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::Manager;

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/LLMxPM/ChatVault/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/LLMxPM/ChatVault/releases";
/// 仅允许从本仓库 GitHub Release 下载资产，避免前端传入任意 URL。
const RELEASE_DOWNLOAD_PREFIX: &str = "https://github.com/LLMxPM/ChatVault/releases/download/";
const USER_AGENT: &str = concat!("ChatVault-Desktop/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    prerelease: bool,
    draft: bool,
    assets: Vec<GithubAsset>,
}

/// 关于页展示的更新检测结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub release_title: String,
    pub installer_name: Option<String>,
    pub installer_size: Option<u64>,
}

/// 解析 SemVer 主版本；允许带 v 前缀与预发布段。
fn parse_semver(version: &str) -> Option<(u64, u64, u64, Option<String>)> {
    let trimmed = version.trim().trim_start_matches('v');
    let (core, pre) = match trimmed.split_once('-') {
        Some((core, pre)) => (core, Some(pre.to_ascii_lowercase())),
        None => (trimmed, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, pre))
}

/// 判断 latest 是否比 current 更新；同数字版本时正式版高于预发布。
fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(latest_v), Some(current_v)) => {
            if latest_v.0 != current_v.0 {
                return latest_v.0 > current_v.0;
            }
            if latest_v.1 != current_v.1 {
                return latest_v.1 > current_v.1;
            }
            if latest_v.2 != current_v.2 {
                return latest_v.2 > current_v.2;
            }
            matches!((&latest_v.3, &current_v.3), (None, Some(_)))
        }
        _ => latest.trim().trim_start_matches('v') != current.trim().trim_start_matches('v'),
    }
}

/// 从资产列表挑选 Windows NSIS 安装包；忽略校验文件与非 exe。
fn pick_installer(assets: &[GithubAsset]) -> Option<&GithubAsset> {
    assets
        .iter()
        .filter(|asset| asset.name.ends_with("-setup.exe") && asset.size > 0)
        .min_by(|a, b| a.name.cmp(&b.name))
}

fn release_version_from_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('v').to_string()
}

/// 拉取 GitHub latest Release（不含预发布）并与当前版本比较。
async fn fetch_latest_release() -> Result<GithubRelease, String> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("创建更新检查客户端失败：{error}"))?;
    let response = client
        .get(LATEST_RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("连接 GitHub 失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub 返回错误状态：HTTP {}",
            response.status().as_u16()
        ));
    }
    let release: GithubRelease = response
        .json()
        .await
        .map_err(|error| format!("解析 GitHub Release 失败：{error}"))?;
    if release.draft || release.prerelease {
        return Err("最新 Release 不可用（草稿或预发布）".to_string());
    }
    Ok(release)
}

/// 检查是否有可升级的正式版本。
#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();
    let release = fetch_latest_release().await?;
    let latest_version = release_version_from_tag(&release.tag_name);
    let installer = pick_installer(&release.assets);
    Ok(UpdateCheckResult {
        has_update: is_newer_version(&latest_version, &current_version),
        current_version,
        latest_version,
        release_url: if release.html_url.is_empty() {
            RELEASES_PAGE.to_string()
        } else {
            release.html_url
        },
        release_title: release
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| release.tag_name.clone()),
        installer_name: installer.map(|asset| asset.name.clone()),
        installer_size: installer.map(|asset| asset.size),
    })
}

fn is_allowed_release_download(url: &str) -> bool {
    url.starts_with(RELEASE_DOWNLOAD_PREFIX)
        && url
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "/.?&=%-_+~".contains(ch))
}

fn parse_sha256_asset(content: &str, expected_name: &str) -> Result<String, String> {
    // GNU 风格校验文件：`<hash>  <filename>`
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let hash = parts.next().unwrap_or_default().to_ascii_lowercase();
        let name = parts.next().unwrap_or_default();
        if hash.len() == 64
            && hash.chars().all(|ch| ch.is_ascii_hexdigit())
            && (name == expected_name || name.ends_with(expected_name))
        {
            return Ok(hash);
        }
    }
    Err("校验文件中未找到匹配的 SHA256".to_string())
}

fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("读取安装包失败：{error}"))?;
    let digest = Sha256::digest(&bytes);
    let actual = hex::encode(digest);
    if actual != expected.to_ascii_lowercase() {
        return Err(format!(
            "安装包 SHA256 校验失败：期望 {expected}，实际 {actual}"
        ));
    }
    Ok(())
}

async fn download_to_path(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
) -> Result<(), String> {
    if !is_allowed_release_download(url) {
        return Err("下载地址不在允许的 GitHub Release 域名内".to_string());
    }
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("下载失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("下载失败：HTTP {}", response.status().as_u16()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("读取下载内容失败：{error}"))?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建更新目录失败：{error}"))?;
    }
    std::fs::write(destination, &bytes).map_err(|error| format!("保存安装包失败：{error}"))?;
    Ok(())
}

/// 下载最新正式版安装包，校验 SHA256 后启动安装器。
#[tauri::command]
pub async fn download_and_install_update(app: tauri::AppHandle) -> Result<(), String> {
    let release = fetch_latest_release().await?;
    let installer = pick_installer(&release.assets).ok_or("该 Release 未提供 Windows 安装包")?;
    if !is_allowed_release_download(&installer.browser_download_url) {
        return Err("安装包下载地址校验失败".to_string());
    }

    let updates_dir: PathBuf = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("无法定位更新目录：{error}"))?
        .join("updates");
    std::fs::create_dir_all(&updates_dir).map_err(|error| format!("创建更新目录失败：{error}"))?;
    let installer_path = updates_dir.join(&installer.name);

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|error| format!("创建下载客户端失败：{error}"))?;

    tracing::info!("开始下载更新 {}（{} 字节）", installer.name, installer.size);
    download_to_path(&client, &installer.browser_download_url, &installer_path).await?;

    // 优先下载同名 .sha256 并校验；缺失时仍继续，但写入警告。
    let checksum_url = format!("{}.sha256", installer.browser_download_url);
    let checksum_result = client
        .get(&checksum_url)
        .send()
        .await
        .map_err(|error| error.to_string());
    match checksum_result {
        Ok(response) if response.status().is_success() => {
            let text = response
                .text()
                .await
                .map_err(|error| format!("读取校验文件失败：{error}"))?;
            let expected = parse_sha256_asset(&text, &installer.name)?;
            verify_sha256(&installer_path, &expected)?;
            tracing::info!("更新包 SHA256 校验通过");
        }
        _ => {
            tracing::warn!("未取得 SHA256 校验文件，跳过校验：{checksum_url}");
        }
    }

    std::process::Command::new(&installer_path)
        .spawn()
        .map_err(|error| format!("启动安装程序失败：{error}"))?;
    tracing::info!("已启动安装程序：{}", installer_path.display());
    Ok(())
}

/// 用系统浏览器打开正式版发布列表。
#[tauri::command]
pub fn open_release_page() -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(RELEASES_PAGE)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_newer_version, parse_semver, pick_installer, GithubAsset};

    #[test]
    fn test_parse_semver_with_prefix_and_pre() {
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3, None)));
        assert_eq!(
            parse_semver("0.1.0-alpha.1"),
            Some((0, 1, 0, Some("alpha.1".to_string())))
        );
    }

    #[test]
    fn test_is_newer_compares_core_and_stable_over_pre() {
        assert!(is_newer_version("0.2.0", "0.1.0"));
        assert!(is_newer_version("v0.1.1", "0.1.0"));
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(is_newer_version("0.1.0", "0.1.0-alpha.1"));
        assert!(!is_newer_version("0.1.0-alpha.1", "0.1.0"));
    }

    #[test]
    fn test_pick_installer_skips_checksum() {
        let assets = vec![
            GithubAsset {
                name: "ChatVault_0.1.0_x64-setup.exe.sha256".into(),
                browser_download_url: "https://github.com/LLMxPM/ChatVault/releases/download/v0.1.0/x.sha256".into(),
                size: 64,
            },
            GithubAsset {
                name: "ChatVault_0.1.0_x64-setup.exe".into(),
                browser_download_url: "https://github.com/LLMxPM/ChatVault/releases/download/v0.1.0/ChatVault_0.1.0_x64-setup.exe".into(),
                size: 1024,
            },
        ];
        let picked = pick_installer(&assets).expect("should pick installer");
        assert_eq!(picked.name, "ChatVault_0.1.0_x64-setup.exe");
    }
}
