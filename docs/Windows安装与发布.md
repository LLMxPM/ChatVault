# Windows 安装与发布

## 构建安装包

在 Windows x64 环境中，从仓库根目录运行：

```powershell
pnpm release:windows
```

发行脚本执行以下步骤：

1. 通过 `vswhere` 探测 Visual Studio C++ 编译环境，并配置仓库内置的 `libs/win_x64/sqlite3.lib`。
2. 验证 Rust 主机为 `x86_64-pc-windows-msvc`，并检查 Cargo workspace、根包和桌面包版本一致。
3. 使用 `pnpm install --frozen-lockfile` 校验前端依赖，使用 `cargo rustc --release --locked -p chatvault-cli` 及脚本内链接参数编译归档程序，静态链接 VCRuntime、使用系统 UCRT。
4. 将 CLI 复制至打包暂存目录，合并 `tauri.release.conf.json` 后执行 Tauri 发行构建。Tauri 自动调用前端构建。
5. 产出 NSIS 安装器及同名 `.sha256` 校验文件。

默认输出目录是 `target/release/bundle/nsis/`。若配置了 Cargo 输出目录，则以 `cargo metadata` 返回的目录为准。正式发行统一使用上述入口，单独执行前端构建或 Cargo 编译不会得到完整安装包。

构建依赖包括 Node、pnpm、Rust MSVC 工具链、Visual Studio C++ 工具和可用资源编译器。首次打包需要联网下载 Tauri 所需的 NSIS 工具。用户安装应用无需安装上述开发工具。

独立 Rust 工具链可通过 `CHATVAULT_TOOLCHAIN_ROOT` 指定，其中应包含 `cargo` 和 `rustup` 目录。本机的 `C:\codetools\rust` 仅在 PATH 中没有 Cargo 时被自动探测，不写入安装包。使用该目录中的 LLVM RC 时，包装脚本显式指定 UTF-8，以正确编译中文产品名。

## 安装行为

- 产品显示名：**拾文 ChatVault**；发布者：`ChatVault Team`。
- 安装格式：Windows x64 NSIS `.exe`，简体中文，当前用户安装。
- 安装器负责创建开始菜单入口，完成页可选择桌面快捷方式与立即启动。
- WebView2 缺失时联网下载引导程序；当前安装包不承诺缺少 WebView2 的完全离线安装。
- 桌面程序和 `chatvault-cli.exe` 位于同一安装目录，计划任务仅调用该目录内的 CLI。
- 安装和卸载前检查归档进程；本安装目录仍有归档运行时阻止操作，不强行结束归档。
- 生命周期脚本只清理操作路径指向本安装目录的 `ChatVaultScheduledScan` 任务。查询或删除失败会终止安装步骤；删除失败时尝试恢复原任务启用状态。

当前用户安装及计划任务行为仍需在无管理员权限的干净 Windows 环境中完成端到端验收。

## 数据、日志与首次使用

桌面应用通过 Tauri 的 `app_local_data_dir()` 定位数据，Windows 下为：

```text
%LOCALAPPDATA%\com.chatvault.desktop\
  chatvault.db          本地索引和设置
  chatvault.objects\   本地附件副本及缓存
  logs\
    desktop.log          本次启动日志
    desktop.previous.log 上次启动日志
```

目录位置不受程序安装目录、快捷方式工作目录或命令行启动位置影响。设置页展示真实版本与数据目录，并提供日志入口。版本来自桌面 `package.json`，发行脚本负责校验 Rust 和根包版本一致。

开发目录中的旧 `chatvault.db` 不会自动导入或删除。项目不提供历史开发数据迁移；可重新扫描当前来源或按当前数据结构恢复资料库。

空资料库启动后进入微信来源扫描页面，并展示引导：识别来源和扫描 → 按需配置 WebDAV → 设置定时采集目录。未连接云端时仍支持本地检索。来源记录为空时会再次显示引导，可在当前会话中收起。

应用采用单实例运行，重复启动会唤起已有窗口。关闭窗口直接退出桌面应用；已启用的 Windows 计划任务独立运行。当前未加入系统托盘或开机自启。

启动失败时显示原生错误提示；日志系统初始化成功后的错误会记录到日志文件。日志按启动轮换，保留本次和上次启动文件。

## 覆盖安装、卸载与重装

覆盖安装与卸载前请完成当前归档任务。安装钩子会清理指向相应安装目录的系统任务；下一次启动读取保存的定时开关，重新注册当前 CLI 路径。如果注册失败，会写入日志，设置页可查看实际任务状态并重新启用。

卸载默认保留本地索引、附件副本和 Windows 系统凭据。卸载器选择“清理应用数据”时会再明确确认：这会删除本地索引和待上传附件副本。该操作不删除微信原始附件，也不删除 WebDAV 远端归档。

重装后，同结构资料库可继续使用；数据结构发生变化时按开发阶段约束重新初始化或重建，不提供版本迁移分支。

## 图标维护

应用图标已通过内置 imagegen 生成，并接入侧栏、页面图标、EXE 和安装器资源。母版、最终提示词和重新导出命令见 [图标说明](../apps/desktop/src-tauri/icons/README.md)。

当前 ICO 包含 16、24、32、48、64、256 像素图层，另提供 32、128、256 像素 PNG。任务栏、快捷方式和卸载入口由 Tauri/Windows 使用应用图标。

## 验证记录与发布边界

2026-09-11：

- 前端类型检查和构建通过。
- 桌面 Rust `cargo check`、`cargo fmt --all -- --check` 和 `cargo test --workspace --locked` 通过。
- 安装钩子 7 项隔离测试通过：不存在任务、本目录任务、其他目录任务、本目录运行中、其他目录运行中、查询失败、删除失败恢复。测试只使用替身，不操作真实系统任务。
- 已检查 128 像素图标预览和 ICO 图层。
- `pnpm release:windows` 完整通过，生成 `拾文 ChatVault_0.1.0_x64-setup.exe`（5,855,123 字节）及同名 SHA256 文件，校验和已复核。
- 已检查桌面程序与 CLI 的 PE 依赖，均不依赖 `sqlite3.dll` 或 `VCRUNTIME140.dll`；原生程序仅引用 Windows 系统组件，WebView2 由安装器检测处理。
- 随包 CLI 暂存副本与发行 CLI 的 SHA256 一致，NSIS 生成脚本包含该资源。
- 发行桌面程序启动冒烟检查通过：从 `target/` 工作目录启动，在固定用户目录创建数据库和日志；窗口标题正确。再从仓库根目录启动，第二个进程正常退出、原实例仍存活；关闭窗口后原实例正常退出。

本次安装包 SHA256：`42ac5196d4e5e70a06056759b7acc5c0ccd9a089ac347eb720e7df14de8a006d`。重新构建后以随产物生成的校验文件为准。

尚待真实环境验收：干净系统首次安装、WebView2 缺失、普通用户权限、实际安装/卸载、覆盖安装、不同启动入口、100%/150%/200% 缩放。

当前未配置代码签名证书、公开下载渠道或自动更新。安装器属于本地测试发行产物；签名和对外分发需要实际证书与发布渠道。MSI、ARM64、完全离线安装器和 macOS 分发仍不在本次实现范围。

参考：[Tauri Windows 安装器](https://v2.tauri.app/distribute/windows-installer/)、[配置参考](https://v2.tauri.app/reference/config/)。
