---
feature: frontend-ux-redesign
status: delivered
updated: 2026-09-12
branch: main
commits: 815365b..WORKTREE # 未提交，工作区交付
---

# 前端交互体系与双主题重构

## Report

**What was built** — 将桌面前端从「深色硬编码 + 7 项混乱导航」重构为工具软件气质的双主题体系：CSS 变量 token（浅色默认，深色/跟随系统可切换）、五项导航（文件库/采集/归档/任务/设置）、Ui* 基础组件与 toast/confirm 反馈层。WebDAV 配置收敛到设置页，采集合并扫描与来源标注，扫描完成后可一键去检索，恢复资料库需二次确认，任务列表 5 秒轮询且失败防刷屏。

**Verification** — `pnpm check:frontend` PASS；`pnpm build:frontend` PASS；Playwright + Edge 截图验证浅色/深色文件库、采集、设置页（`output/playwright/final-*.png`）。独立 review 无 Critical；4 个 Major 已修复（采集目录并入扫描且展示持久化目录、移除 setAppSettings 局部回写、Archive 不再 saveWebdavConfig、任务轮询 silent+inflight）。

**Journey log**
1. Vite 旧进程缓存导致 token 类未生成、主按钮透明——需重启 dev server 再验视觉。
2. `set_app_settings` 会重注册计划任务并回收缓存，前端局部写回不可用；采集源由任务页独立维护并按适配器类型持久化。
3. 后端 `run_scan` 读取带类型的 `collect_sources`，前端不再维护目录字符串与适配器的双重配置。
4. 工具参数对 `\x`、`\u0000`、反引号敏感，大段 Vue 源码宜用 write 工具或分片落盘。
5. PowerShell 解析错误会整段不执行，修复需逐文件确认落盘。

## [S1] Problem

桌面端前端存在四类用户可见问题：

1. **信息架构断裂**：导航 7 项边界不清——「微信来源与扫描」「来源管理」「设置·采集目录」三处都碰来源；WebDAV 配置在 SyncView 与 SettingsView 各写一套且会静默互相覆盖。
2. **AI 生成感**：约 222 处硬编码 `slate-*`/`emerald-*`；无基础组件（输入框/按钮/卡片/刷新各写各的）；字号几乎全是 `text-xs`/`text-[10px]`/`text-[11px]`；emoji 与 lucide 混用；Stats 渐变卡等模板痕迹明显。
3. **只有深色硬编码**：`darkMode: "class"` 已配置但 `index.html` 写死 `class="dark"`，无浅色主题、无系统跟随、无用户切换；`brand` 色板定义后 0 引用。
4. **交互反馈缺失**：`alert` 打断桌面流；扫描完成后不引导去检索；「恢复资料库」无确认；无 toast；任务无自动刷新。

## [S2] Design

### S2.1 设计气质

参考 Linear / Raycast / Notion 桌面端：中性石墨底 + 靛蓝强调、紧凑密度、无渐变装饰、无 emoji 分类、字重与间距做层级。工具感优先，不追求仪表盘或营销卡片。

### S2.2 主题与 Token

采用 CSS 变量 + `html.light` / `html.dark` class 切换（沿用 Tailwind `darkMode: "class"`）。主题偏好三档：`light`（默认）/ `dark` / `system`，持久化到 `localStorage`（key: `chatvault.theme`），设置页可切换；`system` 时监听 `prefers-color-scheme`。

**语义 Token（CSS 变量）**：

| Token | Light | Dark | 用途 |
| --- | --- | --- | --- |
| `--cv-bg` | `#f4f4f5` | `#09090b` | 窗口画布 |
| `--cv-surface` | `#ffffff` | `#141417` | 卡片/面板 |
| `--cv-surface-2` | `#f8f8f9` | `#1c1c21` | 次级底、表头 |
| `--cv-border` | `#e4e4e7` | `#27272a` | 边框/分隔 |
| `--cv-text` | `#18181b` | `#f4f4f5` | 主文字 |
| `--cv-text-2` | `#52525b` | `#a1a1aa` | 次文字 |
| `--cv-text-3` | `#a1a1aa` | `#71717a` | 辅助/占位 |
| `--cv-accent` | `#4f46e5` | `#818cf8` | 主操作、选中 |
| `--cv-accent-fg` | `#ffffff` | `#0b0b12` | 强调底上的字 |
| `--cv-accent-soft` | `#eef2ff` | `#1e1b4b` | 选中底/软强调 |
| `--cv-danger` | `#dc2626` | `#f87171` | 危险操作 |
| `--cv-success` | `#16a34a` | `#4ade80` | 成功态 |
| `--cv-warning` | `#d97706` | `#fbbf24` | 警告态 |

Tailwind `extend.colors` 映射为 `cv-*`。圆角：控件 6px，卡片 8px。字号：页 20 / 区 14 / 正文 13 / 辅助 12；`text-[10px]` 仅徽章。

### S2.3 信息架构（5 项导航）

| 导航 | id | 职责 |
| --- | --- | --- |
| 文件库 | `library` | 检索、筛选、分页、文件操作；顶部摘要库统计 |
| 采集 | `collect` | 来源标注与扫描；Tab「来源标注」改名收藏 |
| 归档 | `archive` | WebDAV 摘要、测试、上传、发布/拉取/恢复 |
| 任务 | `tasks` | 队列、重试/暂停、5s 轮询 |
| 设置 | `settings` | 身份、定时、WebDAV、缓存、外观、存储、关于 |

边界：WebDAV 配置只在 Settings；Archive 只读摘要+操作。采集源以任务页的 `collect_sources` 为准；立即扫描只临时选择微信账号，不增加隐式目录。存储看板独立导航已取消。

### S2.4 基础组件

`apps/desktop/src/components/ui/`：UiButton / UiInput / UiSelect / UiTextarea / UiCard / UiBadge / UiToast+useToast / UiConfirm+useConfirm。图标统一 lucide。

### S2.5 交互契约

1. 禁止 `window.alert`；成功/失败 toast；破坏性操作 confirm。
2. 扫描完成 toast +「去检索」→ library。
3. 任务 5s 轮询；silent 失败 + 60s 节流警告 + inflight 守卫。
4. 恢复资料库 confirm。
5. 主题三档即时生效并持久化。

## [S3] Out of Scope

- 不改 Rust/Tauri 命令与数据模型（除前端已调用 DTO）。
- 不引入重型 UI 框架；不做虚拟滚动、多 Vault、i18n。
- 不改扫描算法与同步协议。

## Tasks

- [x] T1: 建立 CSS 变量与 Tailwind token，更新 index.html/theme 入口 — acceptance: 全局无 slate/emerald 硬编码，html 支持 light/dark（covers: S2.2）
- [x] T2: 实现 useTheme 并接入 Settings 外观区 — acceptance: 三选一切换即时生效且刷新保持（covers: S2.2）
- [x] T3: 实现 UiButton/UiInput/UiSelect/UiTextarea/UiCard/UiBadge — acceptance: 组件可被视图引用，typecheck 通过（covers: S2.4）
- [x] T4: 实现 useToast/UiToast 与 useConfirm/UiConfirm — acceptance: 全局可 toast/confirm，src 无 window.alert（covers: S2.4; S2.5）
- [x] T5: 重构 App 导航为 5 项 — acceptance: 侧栏仅 5 项，firstRun 进入 collect（covers: S2.3）
- [x] T6: LibraryView 接入 token 与基础组件 — acceptance: 检索可用且无 alert（covers: S2.2; S2.5）
- [x] T7: 合并 Scanner+SourceManagement 为 CollectView — acceptance: 扫描合并设置采集目录并可跳 Library（covers: S2.3; S2.5）
- [x] T8: ArchiveView 仅保留操作与状态 — acceptance: WebDAV 表单只在 Settings；恢复 confirm（covers: S2.3; S2.5）
- [x] T9: TasksView 轮询 + toast + token 化；Stats 入口移除 — acceptance: 任务自动刷新且失败不刷屏（covers: S2.3; S2.5）
- [x] T10: SettingsView 统一配置与主题 — acceptance: 设置页可完成全部配置（covers: S2.3; S2.5）
- [x] T11: 全局清扫与构建 — acceptance: typecheck+build 绿（covers: S2.2; S2.5）
