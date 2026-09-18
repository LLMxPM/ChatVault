---
feature: collect-source-path-normalize
status: delivered
updated: 2026-09-18
branch: main
commits: b3819af..working-tree
---

# 采集源路径规范化与账号勾选一致性

## Report

**What was built** — 采集源落库路径统一去掉 Windows `\\?\` 扩展前缀；`set_collect_sources` 返回规范化列表；前端 `normalizePath` 与后端 `normalize_scan_key` 对齐，保存成功后按序号回写 path / 账号 sourceRoot / 勾选 / 快照，勾选键仍为 `(sourceRoot, accountId)`。保存链路改为：先 `filterSelectedAccounts` 只改内存，`persistSources` 成功后再写勾选库；回滚快照 `cloneSources` 重置 `inspecting`，`pickAndReplaceSource` finally 按当前下标清标志。

**Verification** — `cargo fmt --all -- --check` PASS；`pnpm check:rust` PASS；`pnpm check:frontend` PASS（含 critical 修复后复跑）；`cargo test` settings 路径规范化与采集源校验单测 PASS。

**Journey log** —
1. 首次添加后重进任务页丢勾选，根因是后端 canonicalize 落库 `\\?\` 而前端键不剥前缀。
2. 保存命令原先不返回规范化路径，前端内存与库内长期分叉。
3. 勾选回写曾短暂按裸 accountId 映射，审查指出跨源同 id 风险后改为按旧 root 键替换。
4. 审查发现 `replace`/`remove` 在 `persistSources` 前 `syncSelectedAccounts` 会先写库，失败回滚只恢复内存；改为保存成功后再落勾选。
5. `cloneSources` 曾把 `inspecting: true` 拷进回滚快照，失败后 UI 卡 loading；快照重置标志并按列表下标清理。

## [S1] Problem

首次添加微信/企业微信/附件目录后，离开任务页再回来，账号勾选会丢失。

根因是路径形态在保存链路两端不一致：

1. 后端 `set_collect_sources` 对路径 `canonicalize`，Windows 上落库为 `\\?\C:\...`。
2. 前端内存、探测快照、账号 `sourceRoot`、`selectedAccounts` 仍使用对话框返回的普通路径（无 `\\?\`）。
3. 前端 `normalizePath` / `accountKey` 不剥离 `\\?\`，重载后无法与库内路径对齐。
4. `loadSources` 在匹配失败时会把勾选过滤为空并回写，造成持久性丢失。

同会话内「立即运行」往往仍能扫到（后端 `account_selected` 用了 `normalize_scan_key`），但 keep-alive 的 `onActivated → loadSettings` 会触发上述丢失。

## [S2] Design

### 2.1 路径键契约

两侧比对键必须等价于后端 `normalize_scan_key`：

- 统一 `\` 分隔符
- 去掉尾部分隔符
- 小写
- 去掉 `\\?\` / `\\?\UNC\` 扩展前缀（`\\?\UNC\server\share` → `\\server\share`）

### 2.2 落库路径形态

`normalize_collect_source_path` 在 `canonicalize` 成功后**必须**去掉扩展前缀再落库；失败时保留用户原路径。

结果：`collect_sources` 中的 `path` 为普通绝对路径（如 `C:\...\xwechat_files`），与探测快照、账号 `sourceRoot` 同一形态。

### 2.3 保存命令返回规范化列表

`set_collect_sources` 返回 `Vec<CollectSource>`（规范化后的完整列表），前端用返回值替换内存中的 `path` / `sourceType` / `enableVideos`，再写探测快照与账号勾选。

不得只改库、不回写前端内存。

### 2.4 账号勾选键

保持 `(sourceRoot, sourceAccountId)`：

- `sourceRoot` 与对应采集源落库路径一致（保存成功后回写）
- 比较时两侧都走 2.1 的键

同一会话添加后自动全选、重载后保留勾选、立即运行仍只扫勾选账号。

### 2.5 探测快照

快照 `path` 必须与落库 `collect_sources.path` 完全一致，否则首屏秒开失效并触发全量重扫。

保存成功后立即用规范化路径重写快照。

### 2.6 错误与回滚

- `set_collect_sources` 失败：前端恢复 previousSources / previousSelections；勾选过滤不得在保存成功前写库（仅内存过滤）。
- 成功后再 `persistSelectedAccounts` / `persistSourceCache`；写快照/勾选失败：不回滚已保存的采集源；勾选失败已有 toast。
- 回滚快照不得携带 `inspecting: true`；失败后必须能按列表内对象解除 loading。

### 2.7 测试边界

- 后端：`normalize_collect_source_path` 去前缀；重复/重叠仍拒绝；返回值 path 无 `\\?\`。
- 前端逻辑若可抽纯函数则测键一致性；否则以类型检查 + 手工路径对照为准。
- 不改扫描检查点语义、不改适配器探测规则、不做历史库迁移。

## [S3] Out of Scope

- `fullScan` 默认值与首次运行策略（无检查点时已等价全量）。
- 账号勾选持久化改为 await / 竞态加固（可选后续）。
- 侧边栏「已配置采集源」是否要求 ready（仅看 length）。
- 通用目录 `enableVideos` 字段契约整理。
- CLI 定时扫描路径策略（其已用 `normalize_scan_key` 匹配）。
- macOS / 非 Windows 路径。

## Tasks

- [x] T1: 后端 `normalize_collect_source_path` 去掉扩展前缀并保持 canonicalize — acceptance: 落库 path 不含 `\\?\`；失效路径仍原样保留；现有校验单测通过（covers: S2.2）
- [x] T2: `set_collect_sources` 返回规范化后的 `Vec<CollectSource>`，前端命令签名同步 — acceptance: 前端 invoke 拿到与库内一致的 path 列表（covers: S2.3; depends: T1）
- [x] T3: 前端 `normalizePath` 对齐 `normalize_scan_key`（剥 `\\?\` / `\\?\UNC\`）— acceptance: `\\?\C:\a` 与 `C:\a` 生成相同键（covers: S2.1）
- [x] T4: `persistSources` 成功后用返回列表更新内存 path，并回写账号 `sourceRoot` / `selectedAccounts` / 快照 — acceptance: 添加后离开任务页再进入，账号勾选仍在；快照可命中（covers: S2.4, S2.5; depends: T2, T3）
- [x] T5: 回归验证 — acceptance: `cargo fmt --check` 相关范围、`pnpm check:rust` 或至少 `settings` 单测、`pnpm check:frontend` 通过（covers: S2.7; depends: T1–T4）
