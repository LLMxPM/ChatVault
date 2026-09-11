//! ChatVault 本地索引模块构建脚本
//!
//! 在 Windows 平台上自动定位仓库内的 libs/win_x64，确保在未配置系统级环境变量时
//! 依然能顺利链接包含 FTS5 全文索引的 SQLite 原生库，提升开箱即用与跨机器可移植性。

use std::path::Path;

fn main() {
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let repo_libs = Path::new(&manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("libs").join("win_x64"));

        if let Some(libs_dir) = repo_libs {
            if libs_dir.exists() {
                println!("cargo:rustc-link-search=native={}", libs_dir.display());
            }
        }
    }
}
