# 项目协作规范

本文件适用于整个 ChatVault（拾文）仓库。

## 项目概况

ChatVault 是桌面端聊天附件归档与检索工具，当前优先支持 Windows 微信/企业微信附件采集、本地检索、WebDAV 归档及多设备同步恢复。项目采用 Rust Cargo workspace 与 pnpm workspace；桌面端使用 Tauri 2、Vue 3、TypeScript 和 Vite，本地索引使用 SQLite，远端存储使用 WebDAV。

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
| `docs/` | 产品与实施规划、架构与同步设计、开发构建与安装发布说明、功能规格。 |
| `scripts/` | 构建等辅助脚本。 |
| `libs/` | 本地原生依赖，目前包含 Windows x64 SQLite 链接库。 |

根目录 `Cargo.toml` 管理 Rust workspace 与共享依赖，`pnpm-workspace.yaml` 和 `package.json` 管理前端 workspace 与常用命令。`target/`、`node_modules/` 属于构建产物或依赖目录。

## 开发阶段与兼容性约束

- 项目当前处于开发阶段，不需要任何面向历史版本或假设中的未来版本的兼容性设计，也不承诺向前或向后兼容。
- 修改接口、配置、数据模型、本地数据库结构或 WebDAV 存储格式时，直接采用当前设计，并同步更新所有调用方、测试和相关文档。
- 不新增为兼容旧实现而存在的兼容层、旧接口别名、双格式读写、版本分支、历史数据迁移或降级回退逻辑，也不为假设中的未来版本预留兼容分支。
- 修改涉及的模块中若已有上述兼容性设计，应直接删除，同时清理对应的废弃代码、配置、测试及文档描述，不保留过渡实现。
- 开发数据可通过重新初始化、重建索引或重新导入适配当前结构，无需为保留历史开发数据设计升级路径。删除兼容逻辑不等于直接删除用户原始附件或远端归档数据。
- 正常的错误处理、完整性校验、上传重试、离线支持和同步恢复属于业务可靠性要求，不能以取消兼容为由删除。

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
- 更改架构、数据格式或业务行为时，同步维护 `docs/` 中对应文档。
