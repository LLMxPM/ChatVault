// ChatVault 前端与 Tauri 2 后端通讯封装
// 提供类型安全的 invoke 方法，包含微信探测、扫描、搜索、文件定位与 WebDAV 操作

import { invoke } from "@tauri-apps/api/core";
import type {
  WechatAccountDto,
  ScanRequestDto,
  ScanResultDto,
  FileRecordViewDto,
  SearchQueryDto,
  VaultStatsDto,
  WebdavConfigDto,
  WebdavCapabilityDto,
  ArchiveResultDto,
  AppSettingsDto,
  UploadTaskDto,
  SyncResultDto,
} from "../types";

/**
 * 探测本机微信 4.x 账号列表
 * @returns 微信账号及附件统计列表
 */
export async function detectWechatAccounts(): Promise<WechatAccountDto[]> {
  return await invoke<WechatAccountDto[]>("detect_wechat_accounts");
}

/**
 * 执行文件库扫描与增量去重
 * @param request 扫描目标配置
 * @returns 扫描执行总结报告
 */
export async function runScan(request: ScanRequestDto): Promise<ScanResultDto> {
  return await invoke<ScanResultDto>("run_scan", { request });
}

/**
 * 全文搜索与多维筛选文件记录
 * @param query 查询过滤条件
 * @returns 匹配的文件视图列表
 */
export async function searchRecords(query: SearchQueryDto): Promise<FileRecordViewDto[]> {
  return await invoke<FileRecordViewDto[]>("search_records", { query });
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
 * 将本地对象归档推送到 WebDAV 并执行流式校验
 * @param config WebDAV 连接配置
 * @returns 归档总结报告
 */
export async function archiveToWebdav(config: WebdavConfigDto): Promise<ArchiveResultDto> {
  return await invoke<ArchiveResultDto>("archive_to_webdav", { config });
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
 * 查询系统计划任务是否已注册
 */
export async function getScheduleStatus(): Promise<boolean> {
  return await invoke<boolean>("get_schedule_status");
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
 * 发布本机元数据日志
 */
export async function syncPublish(config: WebdavConfigDto): Promise<SyncResultDto> {
  return await invoke<SyncResultDto>("sync_publish", { config });
}

/**
 * 拉取远端日志并合并
 */
export async function syncPull(config: WebdavConfigDto): Promise<SyncResultDto> {
  return await invoke<SyncResultDto>("sync_pull", { config });
}

/**
 * 从远端恢复索引
 */
export async function syncRestore(config: WebdavConfigDto): Promise<SyncResultDto> {
  return await invoke<SyncResultDto>("sync_restore", { config });
}

/** 保存同步页连接参数，和设置页及计划任务共用。 */
export async function saveWebdavConfig(config: WebdavConfigDto): Promise<void> {
  return invoke<void>("save_webdav_config", { config });
}

/** 读取完整索引中的来源账号，独立于列表分页。 */
export async function listRecordAccounts(): Promise<string[]> {
  return invoke<string[]>("list_record_accounts");
}
