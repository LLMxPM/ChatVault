# 项目协作规范

本文件适用于整个 ChatVault（拾文）仓库。

## 项目概况

ChatVault 是桌面端聊天附件归档与检索工具，当前优先支持 Windows 微信/企业微信附件采集、本地检索、WebDAV 归档及多设备同步恢复。项目采用 Rust Cargo workspace 与 pnpm workspace；桌面端使用 Tauri 2、Vue 3、TypeScript 和 Vite，本地索引使用 SQLite，远端存储使用 WebDAV。

开源许可：AGPL-3.0-or-later（见根目录 `LICENSE`）。Cargo workspace、npm 包与 README 中的许可声明须保持一致。

当前阶段：Windows 桌面端功能开发阶段性结束，进入发布准备（安装包、验收与文档核对）。macOS 适配、标签/收藏/会话别名等仍属后续阶段，不在本次发布范围。

## 项目结构

| 路径 | 职责 |
| --- | --- |
| `apps/desktop/src/` | Vue 前端；`views/` 存放页面，`api/` 封装 Tauri 调用，`types/` 定义前端类型。 |
| `apps/desktop/src-tauri/` | Tauri 桌面宿主，负责命令接口、应用状态、连接与任务调度；`src/commands/` 按业务拆分命令。 |
| `crates/core/` | 共享领域模型与错误定义。 |
| `crates/metadata/` | 文件哈希、元数据格式、校验与暂存。 |
| `crates/scanner/` | 文件遍历、扫描与文件稳定性检测。 |
| `crates/index/` | SQLite 数据结构、索引与查询、设置、任务及上传队列。 |
| `crates/scan/` | 采集源扫描编排：按来源类型分发适配器、合并候选、入库与检查点；桌面与 CLI 共用。 |
| `crates/webdav/` | WebDAV 客户端、能力检测、远端存储与发布校验。 |
| `crates/sync/` | 归档、同步发布、远端变更应用与恢复；`tests/` 包含恢复测试。 |
| `crates/cli/` | 命令行入口、参数解析与同步命令。 |
| `adapters/wechat-windows/` | Windows 微信 4.x 来源适配：目录/账号探测、附件与视频媒体根解析。 |
| `adapters/wxwork-windows/` | Windows 企业微信来源适配：WXWork 根/账号探测、Cache/File 与 Cache/Video 解析。 |
| `adapters/generic-folder/` | 通用文件夹来源适配。 |
| `docs/` | 产品与实施规划、架构与同步设计、开发构建与安装发布说明；`docs/compose/spec/` 为已实现功能的结论性契约。 |
| `scripts/` | 构建等辅助脚本。 |
| `libs/` | 本地原生依赖，目前包含 Windows x64 SQLite 链接库。 |

根目录 `Cargo.toml` 管理 Rust workspace 与共享依赖，`pnpm-workspace.yaml` 和 `package.json` 管理前端 workspace 与常用命令。`target/`、`node_modules/` 属于构建产物或依赖目录。

## 文档地图

| 文档 | 用途 |
| --- | --- |
| [产品与实施规划](docs/产品与实施规划.md) | 产品定位、功能范围、分阶段交付与验收目标。 |
| [架构与同步设计](docs/架构与同步设计.md) | 模块边界、数据模型、WebDAV 格式、扫描/归档/同步与恢复。 |
| [开发与构建](docs/开发与构建.md) | 本地依赖、开发命令、Windows 构建与 GitHub Actions。 |
| [Windows 安装与发布](docs/Windows安装与发布.md) | 安装包、数据位置、首次使用、卸载与发布边界。 |
| `docs/compose/spec/*.md` | 单项功能的历史结论契约（status: implemented/delivered），实现细节以代码与上述主文档为准。 |

改架构、数据格式、同步协议或业务行为时，同步更新对应主文档；已实现规格一般不必再改，除非契约与实现再次分叉。

## 版本与兼容性约束

- 发行版本真值为根目录 `package.json` 的 `version`（当前 `0.1.0`）；Cargo workspace 与桌面包版本必须一致，由 `scripts/validate_versions.ps1` 校验。打 `vX.Y.Z` 或 `vX.Y.Z-<预发布>`（如 `v0.1.0-alpha.1`）tag 前先对齐版本；预发布版本号需包含 `-` 后缀，发布工作流会将其标为 GitHub Pre-release。
- 项目尚未对外承诺数据格式稳定性：不为历史开发库、旧 WebDAV 结构或假设中的未来版本增加兼容层、双格式读写或迁移分支。
- 修改接口、配置、数据模型、本地数据库结构或 WebDAV 存储格式时，直接采用当前设计，并同步更新所有调用方、测试和相关文档。
- 修改涉及的模块中若仍有上述兼容设计，应直接删除，同时清理废弃代码、配置、测试及文档描述。
- 开发数据可通过重新初始化、重建索引或重新导入适配当前结构。删除兼容逻辑不等于删除用户原始附件或远端归档数据。
- 正常的错误处理、完整性校验、上传重试、离线支持和同步恢复属于业务可靠性要求，不能以取消兼容为由删除。
- 对外发布后再破坏本地库或 WebDAV 格式，需在发布说明中明确，并评估是否引入迁移；在此之前仍按上述规则直接改结构。

## 基础规范

- 使用中文进行协作和文档编写。
- 控制单个代码文件的行数；文件过长或职责复杂时，按功能拆分模块。
- 每个源代码文件开头应包含文件功能描述，Markdown 文件除外。
- 为函数补充中文注释，优先解释职责、输入输出和关键约束。
- 前端使用 pnpm 进行包管理，维护 `pnpm-lock.yaml`，不引入 npm 或 Yarn 锁文件。
- 业务逻辑放在相应 Rust crate 或来源适配器中，Tauri 命令层负责桌面接口编排，前端通过统一 API 封装调用。

## 验证与常用命令

- 根据改动范围执行必要的检查；仅修改文档时检查内容与差异即可。
- Rust 格式检查：`cargo fmt --all -- --check`。
- Rust 检查、Lint 和测试：`pnpm check:rust`、`pnpm lint:rust`、`pnpm test:rust`；这些命令会先校验标准 Windows x64 构建环境。
- 前端类型检查与构建：`pnpm check:frontend`、`pnpm build:frontend`。
- 前端开发服务：`pnpm dev:web`；完整 Tauri 桌面开发：`pnpm dev:desktop`。
- 桌面端 Rust 编译：`pnpm build:desktop`；安装钩子隔离测试：`pnpm test:installer`。
- Windows 安装包：`pnpm release:windows`（产出 NSIS setup 与 `.sha256`）。
- 发布相关改动至少跑通：前端检查、Rust 检查/Lint/测试、安装钩子测试；触及打包流程时再执行 `pnpm release:windows`。
