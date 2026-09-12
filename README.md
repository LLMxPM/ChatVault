# ChatVault（拾文）

把散落在聊天里的文件，变成属于自己的长期资料库。

ChatVault 是桌面端聊天附件归档与检索工具。优先实现 Windows 微信附件采集、本地检索、WebDAV 归档和多台电脑之间的同步恢复，随后适配 macOS。

- [产品与实施规划](docs/产品与实施规划.md)：产品定位、桌面功能、技术栈、阶段任务及验收标准。
- [架构与同步设计](docs/架构与同步设计.md)：模块划分、数据模型、WebDAV 格式、同步与恢复。
- [Windows 安装与发布](docs/Windows安装与发布.md)：安装包构建、数据位置、首次使用、卸载和验证记录。
- [Windows 安装与发布梳理](docs/Windows安装与发布梳理.md)：初始问题清单及后续发布工作。

Storage 采用 WebDAV；本地 SQLite 保存索引，文件暂存和缓存支持上传重试、离线检索及按需取回。

Windows x64 安装包构建：`pnpm release:windows`。`pnpm build:desktop` 只检查并构建前端。
