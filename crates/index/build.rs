//! ChatVault 本地索引模块构建脚本
//!
//! 在 Windows 平台上校验并定位仓库内的 libs/win_x64，确保使用包含 FTS5 全文索引的 SQLite 原生库。

use std::path::Path;

fn main() {
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let repo_libs = Path::new(&manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("libs").join("win_x64"));

        let Some(libs_dir) = repo_libs else {
            panic!("无法定位仓库 SQLite 静态库目录");
        };
        let sqlite_library = libs_dir.join("sqlite3.lib");
        if !sqlite_library.is_file() {
            panic!("缺少 Windows SQLite 静态库：{}", sqlite_library.display());
        }
        println!("cargo:rerun-if-changed={}", sqlite_library.display());
        println!("cargo:rustc-link-search=native={}", libs_dir.display());
    }
}
