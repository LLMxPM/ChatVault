---
feature: wechat-4x-media-backup
status: implemented
updated: 2026-09-14
---

# 微信 4.x 媒体备份（视频）

本文描述微信 4.x 视频媒体备份的范围、扫描策略、数据契约与实施约束。聊天图片解密备份已移除，不再支持。

## 1. 目标与边界

### 1.1 目标

在现有 `msg/file` 文档附件之外，采集：

1. **视频本体**：`msg/video/**/*.mp4`，明文直接采集。

验收核心：新设备没有微信时，能从 Vault 恢复已成功备份的视频。

### 1.2 硬约束（全程不变）

- **不访问微信进程内存**：不做内存扫描、转储、调试附加、注入、Hook。
- **不解密聊天数据库**：不读 `db_storage`、消息表，不用其反推会话名。
- **不改写或删除微信原文件**。

### 1.3 明确不做

- 语音、Rec / Ann / History、表情、头像、朋友圈。
- 聊天图片解密备份（`msg/attach`）。
- 视频封面 `.jpg`、`_thumb.jpg`。
- 仅云端存在、本地未下载或已清理的原视频。

## 2. 视频扫描设计

### 2.1 扫描根

- 路径：`xwechat_files/<account>/msg/video`
- 缺失时扫描层按空目录处理，不报错。

### 2.2 采集范围开关

采集源配置携带 `enable_videos`（前端 `enableVideos`）：

| 值 | 行为 |
| --- | --- |
| 默认 / `true` | 扫描 `msg/file` 后继续扫描 `msg/video` |
| `false` | 只扫描 `msg/file`，跳过视频根 |

仅微信采集源使用该字段；通用附件目录忽略。

### 2.3 采集规则

| 项 | 规则 |
| --- | --- |
| 扩展名 | 仅 `.mp4` |
| 空文件 | 跳过 |
| 会话 ID | 首期不解析，保持未知 |
| 来源账号 | 使用账号目录名 |
| 时间 | 文件 mtime |

### 2.4 增量策略

- 独立检查点：与 `msg/file` 分开，各自维护 `scan_roots.last_scan_started_ms`。
- 与附件相同：目录发现 + 已知文件复检（mtime/size 变化或缓存缺失）。
- 检查点代表「本轮发现完成」；未稳定文件不推进检查点。

## 3. 数据契约

| 字段 | 值 |
| --- | --- |
| `source_type` | `wechat-windows-4` |
| `source_account_id` | 账号目录名 |
| `source_conversation_id` | 未知（None） |
| `original_name` | 源文件名 |
| `time_source` | `mtime` |

视频与附件共用普通 `ingest_file` 入库路径：暂存副本（若小于阈值）、BLAKE3 去重、FTS、上传队列与 journal 事件。

## 4. 模块职责

| 模块 | 职责 |
| --- | --- |
| `adapters/wechat-windows` `detector` | 账号探测；暴露 `files_dir`、`video_dir` |
| `adapters/wechat-windows` `parser` | `msg/file` 与 `msg/video` 遍历解析 |
| `crates/index` | 检查点、普通文件入库、上传队列 |
| `apps/desktop` / `crates/cli` | 扫描编排：先附件根，后视频根 |

## 5. 历史说明

聊天图片离线解密（候选队列、参数白名单、V2 解密、明文暂存）因维护成本高于收益已整体移除：包括 `image_candidates` 表、`enable_images` 开关、适配器 `media` 子模块、图片统计与前端开关。
