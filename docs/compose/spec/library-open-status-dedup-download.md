---
feature: library-open-status-dedup-download
status: delivered
updated: 2026-10-11
branch: main
commits: a7ac239..WORKTREE
---

# 文件库：直接打开、位置状态、按内容归集与远端下载

## Report

**What was built** — 文件库列表改为按内容对象折叠：同一哈希只占一行，可展开查看全部来源（含设备）。新增位置徽章（本地/远程/本地+远程/失效），本机可读时提供「打开」（explorer 直开，不经 cmd）与「定位」；仅远程对象提供「下载」。下载目录可在设置中配置，默认 `%USERPROFILE%\Downloads\ChatVault`；下载经 WebDAV 流式拉取并校验 BLAKE3 后原子落盘，不写入 `local_files`。位置推断改为批量 SQL，避免 N+1。

**Verification** — `cargo test -p chatvault-index` PASS（含折叠与外机远程推断）；`cargo test -p chatvault-desktop --bins` PASS（下载目录/重名）；`cargo check -p chatvault-desktop` PASS；`cargo fmt --all -- --check` PASS；`pnpm check:frontend` PASS；`pnpm build:frontend` PASS。`pnpm check:rust` 因本机缺 vswhere 失败（PRE-EXISTING 环境问题）。

**Journey log**
1. 列表分页必须按 object 而非 record，否则折叠后页大小失真。
2. 位置 `remote` 不能在「无本地且无 backed_up/外机」时回落，否则 missing 会诱导无效下载。
3. Windows `cmd /C start` 对含元字符路径有注入面；打开改用 `explorer <path>` 单参数。
4. `source_count` 应统计对象全部有效来源，而非筛选后匹配数，展开才与角标一致。
5. 下载是用户目录导出副本，故意不写 `local_files`，避免污染采集索引与缓存回收。

## [S1] Problem

当前文件库列表按 `file_records` 平铺，无法直接用本地程序打开文件，也不展示本地/远端位置；相同内容多条来源重复占行；仅远程文件既看不到状态也无下载入口。

1. 操作只有「定位」（资源管理器高亮），没有「用系统默认程序打开」。
2. 查询层已有 `upload_status`，`local_files.availability` 也有模型，但前端 DTO 与列表均未展示位置状态。
3. 内容按 `object_id` 去重，列表却按记录一行行展示，重复内容分散。
4. 多设备同步恢复后会出现仅远端可知的记录，但没有单文件下载命令与落点配置。

## [S2] Design

### S2.1 位置状态

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

### S2.2 按内容折叠

- 列表默认 **一行一个内容对象**（`object_id`），展示代表文件名（同 object 下 `file_time` 最新记录）、大小、分类、位置、来源数（对象全部有效来源，非筛选匹配数）。
- 点击行展开该 object 的全部来源（来源账号/聊天、原名、原路径、时间、发现设备）。
- 分页按 object 计数；检索条件仍作用于 record，再汇总到 object。
- 统计条「记录 N · 对象 M」语义保持不变。

### S2.3 直接打开

- 命令 `open_file_with_system(path)`：使用 `explorer.exe <path>` 直接打开（不经 cmd 解析，避免元字符注入）。
- 打开路径优先级：存在的 `original_path` → 存在的 `cache_path`。
- 操作列：有本地可读路径时显示「打开」+「定位」；仅远程时显示「下载」。
- 双击行等于点击「打开」（不可打开时无操作）。

### S2.4 远端下载

- 设置项 `download_dir`（`app_settings`），空则默认 `%USERPROFILE%\Downloads\ChatVault`。
- 设置页提供目录选择（`pick_directory` 带标题参数）。
- 命令 `download_object(object_id, original_name)`：
  1. 读取 WebDAV 配置与凭据（无 URL/密码则明确报错）；
  2. `GET chatvault-xxxx/objects/blake3/xx/yy/<hash>`；
  3. 流式写入下载目录 `.part` 临时文件并计算 BLAKE3，与 object hash 校验；
  4. 原子 rename 为安全文件名（重名则 `name (1).ext`）；
  5. 返回最终绝对路径；失败删除临时文件。
- 下载是用户目录导出副本，**不**改写 `local_files`，不改变资料库位置状态。

### S2.5 接口契约

Tauri 命令：

| 命令 | 输入 | 输出 |
| --- | --- | --- |
| `search_objects` | SearchQueryDto | FileObjectViewDto[] |
| `list_object_sources` | objectId | FileSourceDto[] |
| `open_file_with_system` | path | void |
| `download_object` | objectId, originalName | DownloadResultDto |

`get_app_settings` / `set_app_settings` 增加 `downloadDir: string`。`pick_directory` 增加可选 `title`。

已删除无 UI 调用的 `search_records` 命令与 `FileRecordViewDto`。

### S2.6 错误行为

- 路径不存在：打开/定位返回明确中文错误，前端 toast。
- 未配置 WebDAV URL 或密码：下载失败提示先到设置完成配置。
- 哈希不一致：删除临时文件，报「远端内容校验失败」。
- 下载目录不可写：报错并保留原设置。

## [S3] Out of Scope

- 不改同步协议、上传队列与缓存回收策略。
- 不做批量下载、断点续传、下载进度条（成功/失败 toast 即可）。
- 不把下载副本写入 `local_files` 或触发重新索引。
- 不改 FTS 与分类筛选算法。

## Tasks

- [x] T1: 查询层按 object 分页聚合 + 位置推断 — acceptance: `search_objects` 返回折叠结果与 location，条件筛选仍正确（covers: S2.1; S2.2）
- [x] T2: `list_object_sources` 来源展开查询 — acceptance: 给定 objectId 返回全部有效来源（covers: S2.2）
- [x] T3: `open_file_with_system` 与库操作命令注册 — acceptance: 存在路径可被系统打开；不存在路径报错（covers: S2.3）
- [x] T4: `download_dir` 设置读写 + 设置页目录选择 — acceptance: 设置可保存并回显，默认 Downloads\\ChatVault（covers: S2.4; S2.5）
- [x] T5: `download_object` 从 WebDAV 拉对象并校验落盘 — acceptance: 校验通过写到下载目录并返回路径；失败清理临时文件（covers: S2.4; S2.6）
- [x] T6: LibraryView 折叠列表、位置徽章、打开/下载/定位 — acceptance: 按内容展示，可展开来源，操作与状态匹配（covers: S2.1–S2.4）
- [x] T7: 前端类型与 API 封装、typecheck/build — acceptance: `pnpm check:frontend` 与 `pnpm build:frontend` 通过（covers: S2.5）
- [x] T8: Rust 相关单元/集成测试与检查 — acceptance: 折叠/位置/下载目录/重名有测试；index+desktop 测试通过（covers: S2.1; S2.2; S2.4）
