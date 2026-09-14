---
feature: wxwork-windows-adapter
status: implemented
updated: 2026-09-15
---

# 企业微信 Windows 适配与共享扫描编排

本文描述企业微信（WXWork）Windows 附件采集的目录契约、扫描策略，以及桌面/CLI 共用的扫描编排层。

## 1. 目标与边界

### 1.1 目标

在微信 4.x 之外，采集企业微信本地明文附件：

1. **文档附件**：`Cache/File/YYYY-MM/**`，保留原文件名。
2. **视频本体**：`Cache/Video/YYYY-MM/**/*.mp4`。

验收核心：新设备没有企业微信时，能从 Vault 恢复已成功备份的文件。

### 1.2 硬约束（全程不变）

- **不访问企业微信进程内存**。
- **不解密 `Data/*.db`**（`file.db` 等为加密库，不用于反推会话名）。
- **不改写或删除原文件**。

### 1.3 明确不做

- 聊天图片（`Cache/Image`）、语音（`Cache/Voice`）、Temp 中转目录。
- 微盘 `WeDrive`、邮件、日历、表情。
- 会话 ID 自动提取（月份目录平铺，无会话子目录）。

## 2. 目录布局（实测样本）

```
Documents\WXWork\
├── <数字账号 ID>\
│   ├── Config.cfg
│   ├── Data\                 # 加密 SQLite，不读
│   └── Cache\
│       ├── File\YYYY-MM\     # 明文附件平铺
│       ├── Video\YYYY-MM\    # 明文 .mp4
│       ├── Image\            # 首版不采集
│       └── Temp\             # 跳过
├── Default\ / Global\ / Profiles\ …  # 噪声目录，过滤
```

- 数据根默认 `Documents\WXWork`，用户可改路径；支持手动指定。
- 账号判定：非噪声目录，且存在 `Cache/` 或 `Data/` 或 `Config.cfg`。

## 3. 数据契约

| 字段 | 值 |
| --- | --- |
| `source_type` | `wxwork-windows` |
| `source_account_id` | 数字账号目录名 |
| `source_conversation_id` | 恒未知（None） |
| `original_name` | 源文件名 |
| `time_source` | `mtime` |

## 4. 共享扫描编排（`crates/scan`）

桌面与 CLI 原先各持一份扫描逻辑，已抽取为 `chatvault-scan`：

| 模块 | 职责 |
| --- | --- |
| `dispatch` | 按 `source_type` 分发采集源 |
| `wechat` / `wxwork` / `generic` | 来源专用扫描 |
| `ingest` | 候选合并、稳定性、入库、检查点 |
| `events` | 进度事件（桌面 tracing / CLI println） |

新增来源类型时：在 `core` 增加常量 → 实现 adapter → 在 `crates/scan` 增加分支 → 设置校验与前端文案。

## 5. 模块职责

| 模块 | 职责 |
| --- | --- |
| `adapters/wxwork-windows` | 根探测、账号枚举、File/Video 解析 |
| `crates/scan` | 桌面与 CLI 共用扫描编排 |
| `apps/desktop` / `crates/cli` | DTO 映射、命令签名、取消与进度输出 |
| `crates/index` | 检查点、入库、上传队列（源无关） |
