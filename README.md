# ChatVault（拾文）

把散落在聊天里的文件，变成属于自己的长期资料库。

ChatVault 是桌面端聊天附件归档与检索工具。优先实现 Windows 微信附件采集、本地检索、WebDAV 归档和多台电脑之间的同步恢复，随后适配 macOS。

- [产品与实施规划](docs/产品与实施规划.md)：产品定位、桌面功能、技术栈、阶段任务及验收标准。
- [架构与同步设计](docs/架构与同步设计.md)：模块划分、数据模型、WebDAV 格式、同步与恢复。
- [开发与构建](docs/开发与构建.md)：本地依赖、开发命令、Windows 构建和 GitHub Actions 流程。
- [Windows 安装与发布](docs/Windows安装与发布.md)：安装包构建、数据位置、首次使用、卸载和验证记录。

Storage 采用 WebDAV；本地 SQLite 保存索引，文件暂存和缓存支持上传重试、离线检索及按需取回。

常用命令：`pnpm dev:web` 启动前端，`pnpm dev:desktop` 启动 Tauri，`pnpm check:frontend` 检查前端，`pnpm check:rust`/`pnpm lint:rust` 检查 Rust，`pnpm test:rust` 运行 Rust 测试，`pnpm test:installer` 验证安装钩子，`pnpm build:desktop` 编译桌面端。

Windows x64 安装包构建：`pnpm release:windows`。完整依赖和构建流程见 [开发与构建](docs/开发与构建.md)。
