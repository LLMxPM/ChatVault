---
feature: library-open-status-dedup-download
status: delivered
updated: 2026-10-11
---

# 文件库：直接打开、位置状态、按内容归集与远端下载

本文是文件库交互与查询的结论性契约：列表按内容对象折叠、位置徽章、本机打开、远端下载。

## 1. 位置状态

每条内容对象计算 `location`：

| 值 | 含义 |
| --- | --- |
| `local` | 本机存在可读路径（`original_path` 或 `cache_path`），尚未确认远端归档 |
| `remote` | 本机无可用路径，但内容已 `backed_up` 或来自其他设备同步的记录 |
| `both` | 本机可读且远端已归档 |
| `missing` | 仅有失效本地映射，或无本地映射且无远端归档证据 |

判定：

- **本机可读**：该 object 下任一 `local_files.original_path`/`cache_path` 在磁盘上存在；`open_path` 优先 original，其次 cache。
- **远端归档**：`upload_tasks.object_id = X AND status = 'backed_up'`；或无 `local_files` 行且存在非本机 `device_id` 的记录。
- 批量实现：一页 object_id 三次集合查询（local paths / backed_up / foreign），避免 N+1。
- 展示为「位置」列徽章：本地 / 远程 / 本地+远程 / 失效。

## 2. 按内容折叠

- 列表默认 **一行一个内容对象**（`object_id`），展示代表文件名（同 object 下 `file_time` 最新记录）、大小、分类、位置、来源数（对象全部有效来源，非筛选匹配数）。
- 点击行展开该 object 的全部来源（来源账号/聊天、原名、原路径、时间、发现设备）。
- 分页按 object 计数；检索条件仍作用于 record，再汇总到 object。
- 统计条「记录 N · 对象 M」语义保持不变。

## 3. 直接打开

- 命令 `open_file_with_system(path)`：使用 `explorer.exe <path>` 直接打开（不经 cmd 解析，避免元字符注入）。
- 打开路径优先级：存在的 `original_path` → 存在的 `cache_path`。
- 操作列：有本地可读路径时显示「打开」+「定位」；仅远程时显示「下载」。
- 双击行等于点击「打开」（不可打开时无操作）。

## 4. 远端下载

- 设置项 `download_dir`（`app_settings`），空则默认 `%USERPROFILE%\Downloads\ChatVault`。
- 设置页提供目录选择（`pick_directory` 带标题参数）。
- 命令 `download_object(object_id, original_name)`：
  1. 读取 WebDAV 配置与凭据（无 URL/密码则明确报错）；
  2. `GET chatvault-xxxx/objects/blake3/xx/yy/<hash>`；
  3. 流式写入下载目录 `.part` 临时文件并计算 BLAKE3，与 object hash 校验；
  4. 原子 rename 为安全文件名（重名则 `name (1).ext`）；
  5. 返回最终绝对路径；失败删除临时文件。
- 下载是用户目录导出副本，**不**改写 `local_files`，不改变资料库位置状态。

## 5. 接口契约

Tauri 命令：

| 命令 | 输入 | 输出 |
| --- | --- | --- |
| `search_objects` | SearchQueryDto | FileObjectViewDto[] |
| `list_object_sources` | objectId | FileSourceDto[] |
| `open_file_with_system` | path | void |
| `download_object` | objectId, originalName | DownloadResultDto |

`get_app_settings` / `set_app_settings` 增加 `downloadDir: string`。`pick_directory` 增加可选 `title`。

## 6. 错误行为

- 路径不存在：打开/定位返回明确中文错误，前端 toast。
- 未配置 WebDAV URL 或密码：下载失败提示先到设置完成配置。
- 哈希不一致：删除临时文件，报「远端内容校验失败」。
- 下载目录不可写：报错并保留原设置。

## 7. 边界

- 不改同步协议、上传队列与缓存回收策略。
- 不做批量下载、断点续传、下载进度条（成功/失败 toast 即可）。
- 不把下载副本写入 `local_files` 或触发重新索引。
- 不改 FTS 与分类筛选算法。
