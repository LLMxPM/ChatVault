---
feature: library-enhance
status: implemented
updated: 2026-10-11
---

# 文件库增强：筛选、详情、批量下载与释放空间

本文是文件库本轮增强的结论性契约。覆盖筛选条件、详情侧栏、批量下载与释放本地空间。开发阶段不留兼容层，直接按本文实现。

与既有契约关系：

- 位置状态、按内容折叠、单对象打开/定位/下载、`download_dir` 继续有效（见 `library-open-status-dedup-download.md`）。
- **行内展开来源**由详情侧栏取代，原契约第 2 节展开交互作废。
- 原契约「不做批量下载」的边界由本文第 4 节覆盖。

本轮**不做**：会话筛选 UI、隐藏/软删除入口、标签/收藏对象、断点续传、下载进度条（仅汇总结果）、正文 OCR。

---

## 1. 筛选增强

### 1.1 条件清单

| 条件 | 字段 | 说明 |
| --- | --- | --- |
| 关键词 | `keyword` | 现有 FTS / LIKE，不变 |
| 分类 | `category` | 现有 chips |
| 来源账号 | `sourceType` + `sourceAccountId` | 现有下拉 |
| 时间区间 | `startTime` + `endTime` + `timeField` | 闭区间，RFC3339；见 1.2 |
| 位置 | `location` | `local` \| `remote` \| `both` \| `missing`，可空 |
| 扩展名 | `extensions` | 小写、不含点，OR 语义；可空数组 |
| 排序 | `sort` | 白名单，见 1.3 |

会话筛选不在本轮 UI 暴露；`SearchFilter.source_conversation_id` 保留给后端/测试。

### 1.2 时间字段

| `timeField` | 过滤列 | 默认 |
| --- | --- | --- |
| `file_time` | `file_records.file_time` | **是** |
| `discovered_at` | `file_records.discovered_at` | 否 |

- UI 文案：文件时间 / 发现时间。
- 详情同时展示二者及 `time_source`（含义）。
- 排序始终按当前 `sort` 指定列；默认排序仍为 `file_time DESC`。

### 1.3 排序白名单

| `sort` 值 | SQL 排序 |
| --- | --- |
| `file_time_desc`（默认） | `file_time DESC, object_id` |
| `file_time_asc` | `file_time ASC, object_id` |
| `size_desc` | `size DESC, object_id` |
| `size_asc` | `size ASC, object_id` |
| `name_asc` | `original_name COLLATE NOCASE ASC, object_id` |
| `name_desc` | `original_name COLLATE NOCASE DESC, object_id` |

非法值回退默认。对象折叠后的代表行字段参与排序（与现 `search_objects` 代表行一致）。

### 1.4 查询结果结构

`search_objects` 返回改为带总数的信封（开发期直接改 DTO，不保留旧数组形状）：

```ts
interface ObjectSearchPageDto {
  total: number; // 满足条件的 object 数
  items: FileObjectViewDto[];
}
```

`total` 与分页同一 WHERE + 对象折叠逻辑下的 COUNT，避免前端用 `hasNext` 猜测。

### 1.5 UI（已按实现调整）

- 筛选区：搜索、类型下拉、来源账号、位置、排序；**不做时间区间与扩展名 chip**（后端能力保留）。
- 刷新为纯图标按钮，位于「共 N 个对象 · 本页 M」右侧。
- 已生效条件显示为可删除 chip；无筛选时展示「暂无筛选项」；「清空筛选」恢复默认。
- 底部翻页区提供每页条数（50/100/200/500，默认 100）。
- 空结果：区分「库为空」（去任务）与「筛选过窄」（清空筛选）。

---

## 2. 详情抽屉

### 2.1 交互

- 点击列表行打开右侧**抽屉**（与来源标注同构：全屏遮罩 + 右侧滑出）；**移除表格行内展开**。
- 关闭：遮罩点击、关闭按钮；再次点击其他行切换详情内容。
- 列表仍按内容对象一行一条；详情数据按 `objectId` 拉取。

### 2.2 详情内容

| 区块 | 字段 |
| --- | --- |
| 摘要 | 原文件名、分类、扩展名、格式化大小、位置徽章 |
| 标识 | `hash`（可复制；不展示 objectId） |
| 时间 | 文件时间 + `time_source`；发现时间 |
| 来源 | 与现 `list_object_sources` 相同字段：账号/聊天显示名、原名、路径、设备、`isLocal`、`hasLocalPath`；每条可「定位」（仅本机有路径） |
| 操作 | 打开 / 定位 / 下载（单个）/ 复制哈希（标识区）/ 释放本机缓存 / 删除本机原文件 |

操作可见性沿用现位置规则：有 `openPath` 显示打开+定位；否则可下载时显示下载；「释放本机缓存」按第 5 节前置条件展示或提示。

### 2.3 实现

- 组件 `LibraryDetailPanel.vue`：固定层右侧抽屉，样式对齐 `SourceLabelPanel`。
- 数据：复用 `search_objects` 当前项 + `list_object_sources(objectId)`。
- 列表刷新后若当前详情 `objectId` 不在结果中，自动关闭抽屉。

---

## 3. 多选与批量栏

### 3.1 选择

- 行首复选框；表头全选/取消**当前页** items。
- 选择状态跨翻页**不保留**（翻页清空），避免误批量。
- 选中数 > 0 时出现底部或顶部批量栏。

### 3.2 批量动作

| 动作 | 行为 |
| --- | --- |
| 下载 | 见第 4 节 |
| 释放缓存 | 见第 5 节 A |

---

## 4. 批量下载（导出）

### 4.1 语义

- 将选中的每个内容对象导出为 `download_dir` 下的用户文件副本。
- **本地已可打开的对象同样导出**（从 `open_path` 复制），不跳过。
- **不**改写 `local_files`，**不**改变位置状态，**不**触发重新索引。

### 4.2 单对象策略

| 本机状态 | 行为 |
| --- | --- |
| 存在 `open_path`（original 优先，其次 cache） | 复制到目标文件名；失败记 failed |
| 无本地路径且远端可下载 | 现有 WebDAV GET + BLAKE3 校验 |
| 两者皆不可 | failed，原因「无本地文件且未远端归档」或 WebDAV 配置错误 |

目标文件名：对象代表 `original_name`；重名走 `unique_dest_path`。

### 4.3 执行

- 顺序处理选中对象；单项失败不中断后续。
- 命令：

```
download_objects(object_ids: string[]) -> BatchDownloadResultDto
```

```ts
interface BatchDownloadItemResultDto {
  objectId: string;
  originalName: string;
  status: "ok" | "failed";
  savedPath?: string;
  error?: string;
}
interface BatchDownloadResultDto {
  total: number;
  okCount: number;
  failedCount: number;
  items: BatchDownloadItemResultDto[];
}
```

- 成功 toast：`已导出 N 个，失败 M 个`；失败时可展开看原因（或后续详情）。
- 不做进度条、取消、并发下载（V1）。
- 单对象 `download_object` 保留，供详情使用；批量内部可复用同一 Rust 辅助函数。

---

## 5. 释放本地空间

### 5.1 动作分层

| 级别 | 删什么 | 是否改 DB | 危险 |
| --- | --- | --- | --- |
| **A. 释放缓存** | 受控 staging 内该 object 哈希文件 | `local_files.cache_path = NULL` | 低 |
| **B. 删除本机原文件** | `local_files.original_path` 指向的源文件；同时清 `local_files` 行 | 是 | 高 |
| **C. 全局回收** | 调用现有 `reclaim_cache`；可选「立即回收全部已归档副本」 | 按现有策略 | 低 |

### 5.2 A · 释放缓存（选中/详情）

- 前置：该 object 关联的本机引用对应上传任务均为 `backed_up`，且入库事件已被同步游标覆盖（与 `reclaim_cache` 保护一致）。
- 不满足：该项 failed/skipped，原因「尚未完成远端归档」。
- 成功：删除哈希副本文件，`cache_path` 置空；位置徽章可能从 both/local 变为 remote/missing。

命令：

```
release_object_cache(object_ids: string[]) -> BatchReleaseResultDto
```

```ts
interface BatchReleaseResultDto {
  total: number;
  okCount: number;
  failedCount: number;
  items: { objectId: string; status: "ok" | "failed"; error?: string }[];
}
```

### 5.3 B · 删除本机原文件

- 入口：详情「删除本机原文件」（危险操作，二次确认）。
- 二次确认展示：文件名、完整路径、大小；说明「不会删除 WebDAV 归档」；**强调若尚未成功归档则本机文件将永久丢失**。
- 后端硬门禁（与释放缓存同级）：
  1. 对象本机引用均已 `backed_up`，且对应 `FileRecordAdded` journal 已被同步游标覆盖；
  2. 有采集源配置时，`original_path` 必须落在已配置采集源根目录之下；
  3. `cache_path` 必须落在受控 staging 目录的直接子文件；
  4. 先全量校验再触碰磁盘；任一原文件删除失败时保留该 record 映射以便重试。
- 行为：
  1. 删除 `original_path`（存在才删；NotFound 视为成功清理）；
  2. 若存在受控 `cache_path` 一并删除副本；
  3. 删除该 record 的 `local_files` 行；
  4. 不写 journal（纯本机文件与映射）。
- 可对多选对象批量执行，命令：

```
delete_object_local_files(object_ids: string[]) -> BatchReleaseResultDto
```

- 删除原文件后位置徽章刷新；增量扫描不会再发现该路径。

### 5.4 C · 全局回收

- 入口：文件库工具栏「释放空间」或设置·存储。
- 默认调用 `reclaim_cache()`（按保留天数与容量目标）。
- 可选文案：「立即回收全部已归档缓存」→ 以 `retention=0` 且不因容量短路的方式回收受控副本（仍遵守未归档保护）。
- **永不**删除 `original_path`。

---

## 6. 接口与类型汇总

### 6.1 Tauri 命令

| 命令 | 输入 | 输出 |
| --- | --- | --- |
| `search_objects` | SearchQueryDto（扩展） | ObjectSearchPageDto |
| `list_object_sources` | objectId | FileSourceDto[]（不变） |
| `download_object` | objectId, originalName | DownloadResultDto（不变） |
| `download_objects` | objectIds[] | BatchDownloadResultDto |
| `release_object_cache` | objectIds[] | BatchReleaseResultDto |
| `delete_object_local_files` | objectIds[] | BatchReleaseResultDto |
| `reclaim_cache_now` | （可选 forceAll: bool） | releasedBytes |
| `open_file_with_system` / `reveal_file_in_explorer` | 不变 | 不变 |

### 6.2 SearchQueryDto 增量字段

```ts
interface SearchQueryDto {
  keyword?: string;
  category?: string;
  sourceType?: string;
  sourceAccountId?: string;
  // sourceConversationId 保留，本轮 UI 不传
  sourceConversationId?: string;
  startTime?: string; // RFC3339
  endTime?: string;
  timeField?: "file_time" | "discovered_at"; // 默认 file_time
  location?: "local" | "remote" | "both" | "missing";
  extensions?: string[];
  sort?: string; // 白名单，默认 file_time_desc
  limit?: number;
  offset?: number;
}
```

### 6.3 FileObjectViewDto

字段保持；前端按 `total` 信封读取。推荐增加 `discoveredAt` 与 `timeSource`，便于详情首屏，避免再查。

---

## 7. 错误行为

| 场景 | 行为 |
| --- | --- |
| 释放缓存但未归档 | 该项 skipped/failed，文案「尚未完成远端归档，无法释放缓存」 |
| 删原文件失败（占用/权限） | 该项 failed，返回路径与系统错误 |
| 批量下载部分失败 | 返回逐项结果；toast 汇总 |
| 未配置 WebDAV 且对象仅远程 | 该项 failed，提示去设置 |
| 原文件已不存在再删 | NotFound 视为成功清理映射 |

---

## 8. 边界与不做

- 不改 FTS 分词、trigram 策略、上传队列结构、WebDAV 布局。
- 不新增隐藏/软删除 UI、Tag/对象收藏/显示名（后续再评估）。
- 不做会话级筛选 UI。
- 批量下载不做断点续传、并行、进度事件。
- 释放空间不删除远端对象、不写 journal。

---

## 9. 验收清单

1. 时间切换 file_time / discovered_at，区间结果正确；默认 file_time。
2. 位置/扩展名/排序/总数/清空 chip 可用；筛选过窄有引导。
3. 点击行打开详情，来源完整；无行内展开。
4. 多选批量下载：本地+远端对象均导出到 `download_dir`，重名加后缀，失败可汇总。
5. 释放缓存仅对已归档对象；全局回收不删原文件。
6. 删除本机原文件需二次确认；未完成归档/未发布元数据时后端拒绝；路径越界拒绝；成功后位置徽章正确。
7. 位置徽章在释放/删原文件后刷新正确。

---

## 10. 实施顺序

| 批次 | 内容 |
| --- | --- |
| P1 | 查询扩展（时间字段/位置/扩展名/排序/total）+ 筛选 UI |
| P2 | 详情抽屉，移除展开 |
| P3 | 多选 + `download_objects` |
| P4 | `release_object_cache` + `delete_object_local_files` + 全局回收入口 |

每批完成后跑相关 `pnpm check:frontend` / `pnpm check:rust` / 定向测试。
