// ChatVault 前端与 Tauri 2 后端通讯封装
// 提供类型安全的 invoke 方法，包含微信探测、扫描、搜索、文件定位与 WebDAV 操作

import { invoke } from "@tauri-apps/api/core";
import type {
  WechatAccountDto,
  WechatAccountTargetDto,
  CollectSourceDto,
  CollectSourceCacheDto,
  ScanRequestDto,
  ScanResultDto,
  FileObjectViewDto,
  FileSourceDto,
  DownloadResultDto,
  SearchQueryDto,
  VaultStatsDto,
  WebdavConfigDto,
  WebdavCapabilityDto,
  AppSettingsDto,
  VaultResetReportDto,
  UploadTaskDto,
  SyncResultDto,
  SourceAccountDto,
  SourceConversationDto,
  UpdateSourceAccountDto,
  UpdateSourceConversationDto,
  PipelineRequestDto,
  PipelineResultDto,
  TaskRunDto,
  TaskRunDetailDto,
} from "../types";

/**
 * 探测本机微信 4.x 账号列表
 * @returns 微信账号及附件统计列表
 */
export async function detectWechatAccounts(): Promise<WechatAccountDto[]> {
  return await invoke<WechatAccountDto[]>("detect_wechat_accounts");
}

/** 检查用户选择的微信 4.x 根目录并返回账号列表。 */
export async function inspectWechatDirectory(path: string): Promise<WechatAccountDto[]> {
  return await invoke<WechatAccountDto[]>("inspect_wechat_directory", { path });
}

/**
 * 执行文件库扫描与增量去重
 * @param request 扫描目标配置
 * @returns 扫描执行总结报告
 */
export async function runScan(request: ScanRequestDto): Promise<ScanResultDto> {
  return await invoke<ScanResultDto>("run_scan", { request });
}

/** 立即运行流水线：扫描 → 归档 → 自动同步，与定时任务同构。 */
export async function runPipeline(request: PipelineRequestDto): Promise<PipelineResultDto> {
  return await invoke<PipelineResultDto>("run_pipeline", { request });
}

/** 保存带适配器类型的持久采集源。 */
export async function setCollectSources(sources: CollectSourceDto[]): Promise<void> {
  return await invoke<void>("set_collect_sources", { sources });
}

/** 读取采集源探测快照（首屏秒开）。 */
export async function getCollectSourceCache(): Promise<CollectSourceCacheDto[]> {
  return await invoke<CollectSourceCacheDto[]>("get_collect_source_cache");
}

/** 写入采集源探测快照。 */
export async function setCollectSourceCache(
  cache: CollectSourceCacheDto[],
): Promise<void> {
  return await invoke<void>("set_collect_source_cache", { cache });
}

/** 读取立即运行勾选的微信账号；null 表示从未配置。 */
export async function getCollectSelectedAccounts(): Promise<
  WechatAccountTargetDto[] | null
> {
  return await invoke<WechatAccountTargetDto[] | null>(
    "get_collect_selected_accounts",
  );
}

/** 持久化立即运行勾选的微信账号。 */
export async function setCollectSelectedAccounts(
  accounts: WechatAccountTargetDto[],
): Promise<void> {
  return await invoke<void>("set_collect_selected_accounts", { accounts });
}

/** 保存定时扫描开关与周期。 */
export async function setScheduleConfig(enabled: boolean, intervalMinutes: number): Promise<void> {
  return await invoke<void>("set_schedule_config", { enabled, intervalMinutes });
}

/**
 * 全文搜索与多维筛选文件记录（按内容折叠）
 * @param query 查询过滤条件
 * @returns 匹配的内容对象列表
 */
export async function searchObjects(query: SearchQueryDto): Promise<FileObjectViewDto[]> {
  return await invoke<FileObjectViewDto[]>("search_objects", { query });
}

/** 列出内容对象的全部来源记录。 */
export async function listObjectSources(objectId: string): Promise<FileSourceDto[]> {
  return await invoke<FileSourceDto[]>("list_object_sources", { objectId });
}

/** 用系统默认程序打开本地文件；无扩展名缓存可传真实扩展名生成受控打开副本。 */
export async function openFileWithSystem(path: string, extension?: string): Promise<void> {
  return await invoke<void>("open_file_with_system", { path, extension });
}

/** 从 WebDAV 下载内容对象到下载目录。 */
export async function downloadObject(
  objectId: string,
  originalName: string,
): Promise<DownloadResultDto> {
  return await invoke<DownloadResultDto>("download_object", { objectId, originalName });
}

/**
 * 获取当前文件库的去重与存储统计指标
 * @returns 存储统计对象
 */
export async function getVaultStats(): Promise<VaultStatsDto> {
  return await invoke<VaultStatsDto>("get_vault_stats");
}

/**
 * 在 Windows 文件资源管理器中高亮选中文件
 * @param path 本机绝对路径
 */
export async function revealFileInExplorer(path: string): Promise<void> {
  return await invoke<void>("reveal_file_in_explorer", { path });
}

/**
 * 测试 WebDAV 远端服务连通性与能力
 * @param config WebDAV 连接配置
 * @returns RFC4918 协议能力探测结果
 */
export async function testWebdav(config: WebdavConfigDto): Promise<WebdavCapabilityDto> {
  return await invoke<WebdavCapabilityDto>("test_webdav", { config });
}

/**
 * 保存 WebDAV 密码到系统凭据管理器 (Windows Credential Manager)
 */
export async function saveWebdavCredential(url: string, username: string, password: string): Promise<void> {
  return await invoke<void>("save_webdav_credential", { url, username, password });
}

/**
 * 从系统凭据管理器读取已保存的 WebDAV 密码
 */
export async function loadWebdavCredential(url: string, username: string): Promise<string> {
  return await invoke<string>("load_webdav_credential", { url, username });
}

/** 删除系统凭据管理器中已保存的 WebDAV 密码（不存在时视为成功）。 */
export async function clearWebdavCredential(url: string, username: string): Promise<void> {
  return await invoke<void>("clear_webdav_credential", { url, username });
}

/**
 * 读取应用设置
 */
export async function getAppSettings(): Promise<AppSettingsDto> {
  return await invoke<AppSettingsDto>("get_app_settings");
}

/**
 * 保存应用设置
 */
export async function setAppSettings(settings: AppSettingsDto): Promise<void> {
  return await invoke<void>("set_app_settings", { settings });
}

/**
 * 清空旧 Vault 绑定与同步状态
 */
export async function resetVaultBinding(): Promise<VaultResetReportDto> {
  return await invoke<VaultResetReportDto>("reset_vault_binding");
}

/**
 * 查询系统计划任务是否已注册
 */
export async function getScheduleStatus(): Promise<boolean> {
  return await invoke<boolean>("get_schedule_status");
}

/** 打开系统目录选择对话框；用户取消时返回 null。 */
export async function pickDirectory(title?: string): Promise<string | null> {
  return await invoke<string | null>("pick_directory", { title });
}

/** 检查采集目录当前是否存在且仍为目录。 */
export async function checkDirectory(path: string): Promise<boolean> {
  return await invoke<boolean>("check_directory", { path });
}

/**
 * 列出上传归档任务
 */
export async function listUploadTasks(status?: string, limit?: number): Promise<UploadTaskDto[]> {
  return await invoke<UploadTaskDto[]>("list_upload_tasks", { status, limit });
}

/**
 * 重新入队任务
 */
export async function requeueUploadTask(taskId: string): Promise<void> {
  return await invoke<void>("requeue_upload_task", { taskId });
}

/**
 * 暂停任务
 */
export async function pauseUploadTask(taskId: string): Promise<void> {
  return await invoke<void>("pause_upload_task", { taskId });
}

/**
 * 从远端恢复索引
 */
export async function syncRestore(config: WebdavConfigDto): Promise<SyncResultDto> {
  return await invoke<SyncResultDto>("sync_restore", { config });
}

/** 读取完整索引中的来源账号映射，独立于文件列表分页。 */
export async function listSourceAccounts(): Promise<SourceAccountDto[]> {
  return invoke<SourceAccountDto[]>("list_source_accounts");
}

/** 按来源类型和账号读取来源聊天映射。 */
export async function listSourceConversations(
  sourceType?: string,
  sourceAccountId?: string,
): Promise<SourceConversationDto[]> {
  return invoke<SourceConversationDto[]>("list_source_conversations", {
    sourceType,
    sourceAccountId,
  });
}

/** 更新来源账号的自定义名称和收藏状态。 */
export async function updateSourceAccount(request: UpdateSourceAccountDto): Promise<void> {
  return invoke<void>("update_source_account", { request });
}

/** 更新来源聊天的自定义名称和收藏状态。 */
export async function updateSourceConversation(request: UpdateSourceConversationDto): Promise<void> {
  return invoke<void>("update_source_conversation", { request });
}

/** 列出最近任务运行。 */
export async function listTaskRuns(limit?: number): Promise<TaskRunDto[]> {
  return await invoke<TaskRunDto[]>("list_task_runs", { limit });
}

/** 读取运行详情（阶段 + 明细）。 */
export async function getTaskRunDetail(runId: string): Promise<TaskRunDetailDto> {
  return await invoke<TaskRunDetailDto>("get_task_run_detail", { runId });
}
