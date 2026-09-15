# Windows 安装与发布

## 构建安装包

在 Windows x64 环境中，从仓库根目录运行：

```powershell
pnpm release:windows
```

发行脚本执行以下步骤：

1. 通过 `vswhere` 探测完整的标准 Visual Studio C++/Windows SDK 环境，并配置仓库内置的 `libs/win_x64/sqlite3.lib`。
2. 验证 Rust 主机为 `x86_64-pc-windows-msvc`，并检查 Cargo workspace、根包和桌面包版本一致。
3. 使用 `pnpm install --frozen-lockfile` 校验前端依赖，使用 `cargo rustc --release --locked -p chatvault-cli` 及脚本内链接参数编译归档程序，静态链接 VCRuntime、使用系统 UCRT。
4. 将 CLI 复制至打包暂存目录，合并 `tauri.release.conf.json` 后执行 Tauri 发行构建。Tauri 自动调用前端构建。
5. 产出 NSIS 安装器及同名 `.sha256` 校验文件。

默认输出目录是 `target/release/bundle/nsis/`。若配置了 Cargo 输出目录，则以 `cargo metadata` 返回的目录为准。正式发行统一使用上述入口，单独执行前端构建或 Cargo 编译不会得到完整安装包。

构建依赖包括 Node 22、pnpm 10.30.3、Rust stable MSVC 工具链、Visual Studio C++ 工具、Windows SDK 和可用资源编译器。首次打包需要联网下载 Tauri 所需的 NSIS 工具。用户安装应用无需安装上述开发工具。

本地和 CI 均使用标准 Visual Studio Build Tools、Windows SDK 和 stable Rust MSVC 工具链。仓库不依赖开发机私有工具链或固定安装路径。

## 安装行为

- 产品名：**ChatVault**（安装包与程序文件名使用 ASCII，窗口标题为「拾文」）；发布者：`ChatVault Team`。
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

目录位置不受程序安装目录、快捷方式工作目录或命令行启动位置影响。设置页展示真实版本与数据目录，并提供日志入口。发行版本真值来自根 `package.json`，发行脚本负责校验桌面包和 Rust workspace 版本一致。

开发目录中的旧 `chatvault.db` 不会自动导入或删除。项目不提供历史开发数据迁移；可重新扫描当前来源或按当前数据结构恢复资料库。

空资料库启动后进入任务页，并展示引导：配置采集范围并立即运行 → 按需配置 WebDAV → 启用定时流水线。未连接云端时仍支持本地检索。来源记录为空时会再次显示引导，可在当前会话中收起。

应用采用单实例运行，重复启动会唤起已有窗口。关闭窗口直接退出桌面应用；已启用的 Windows 计划任务独立运行。当前未加入系统托盘或开机自启。

资料库在桌面事件循环启动前初始化，失败时直接返回主入口，在开发终端和 Windows 原生提示框中显示具体原因；日志系统初始化成功后的错误同时写入日志文件。panic 记录到日志的同时保留默认终端输出及调用栈。日志按启动轮换，保留本次和上次启动文件。

## 覆盖安装、卸载与重装

覆盖安装与卸载前请完成当前归档任务。安装钩子会清理指向相应安装目录的系统任务；下一次启动读取保存的定时开关，重新注册当前 CLI 路径。如果注册失败，会写入日志，任务页可查看实际任务状态并重新启用。

卸载默认保留本地索引、附件副本和 Windows 系统凭据。卸载器选择“清理应用数据”时会再明确确认：这会删除本地索引和待上传附件副本。该操作不删除微信原始附件，也不删除 WebDAV 远端归档。

重装后，同结构资料库可继续使用；数据结构发生变化时按开发阶段约束重新初始化或重建，不提供版本迁移分支。

## 应用内升级

设置 → 关于 可检查更新并安装：

- 仅检测 GitHub 上的**正式版** Release（`/releases/latest`），不含 alpha/rc 等预发布。
- 发现新版本后可「下载并安装」：拉取 NSIS 安装包到本地 `updates` 目录，校验 `.sha256`（若提供）后启动安装器。
- 下载地址限制为本仓库 `github.com/LLMxPM/ChatVault/releases/download/`。
- 手动入口与 GitHub [Releases](https://github.com/LLMxPM/ChatVault/releases) 页仍可用；无自动更新服务。

## 图标维护

应用图标已通过内置 imagegen 生成，并接入侧栏、页面图标、EXE 和安装器资源。母版、最终提示词和重新导出命令见 [图标说明](../apps/desktop/src-tauri/icons/README.md)。

当前 ICO 包含 16、24、32、48、64、256 像素图层，另提供 32、128、256 像素 PNG。任务栏、快捷方式和卸载入口由 Tauri/Windows 使用应用图标。

## 发布边界

- 支持正式 tag（`vX.Y.Z`）与预发布 tag（`vX.Y.Z-alpha` / `-alpha.1` / `-rc.1` 等）；预发布在 GitHub 上标记为 Pre-release。
- 预发布发版前需将 `package.json`、桌面包与 Cargo workspace 版本改为与 tag 一致的预发布号。
- 发行源码与安装包遵循 AGPL-3.0-or-later；GitHub Release 应随源码 tag 发布，便于用户获取对应版本源码。
- 当前安装包为本地测试发行产物；尚未配置代码签名证书、公开下载渠道或自动更新。
- 尚待真实环境验收：干净系统首次安装、WebView2 缺失、普通用户权限、实际安装/卸载、覆盖安装、不同启动入口、高 DPI 缩放。
- MSI、ARM64、完全离线安装器和 macOS 分发不在当前范围。
- 重新构建后以随产物生成的 `.sha256` 校验文件为准。

参考：[Tauri Windows 安装器](https://v2.tauri.app/distribute/windows-installer/)、[配置参考](https://v2.tauri.app/reference/config/)。
