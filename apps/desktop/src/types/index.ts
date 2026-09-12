// ChatVault 桌面端通用数据类型定义
// 用于前端与 Rust Tauri 后端通过 invoke 进行交互的契约对象

/**
 * 微信 4.x 账号探测信息
 */
export interface WechatAccountDto {
  accountId: string;
  sourceDir: string;
  filesCountEstimated: number;
}

/**
 * 扫描请求参数
 */
export interface ScanRequestDto {
  targetAccounts: string[];
  customFolders: string[];
  /** true 时忽略检查点做全量发现；默认 false 使用增量扫描 */
  fullScan?: boolean;
}

/**
 * 扫描执行报告
 */
export interface ScanResultDto {
  totalDiscovered: number;
  totalNewObjects: number;
  totalSkipped: number;
  durationMs: number;
}

/**
 * 文件记录视图对象（用于列表呈现与检索结果）
 */
export interface FileRecordViewDto {
  recordId: string;
  objectId: string;
  originalName: string;
  fileSize: number;
  formattedSize: string;
  hash: string;
  sourceType: string;
  accountId?: string | null;
  fileTime?: string | null;
  discoveredAt: string;
  originalPath: string;
  category: "doc" | "image" | "video" | "audio" | "archive" | "other";
}

/**
 * 全文搜索与筛选查询参数
 */
export interface SearchQueryDto {
  keyword?: string;
  category?: string;
  accountId?: string;
  limit?: number;
  offset?: number;
}

/**
 * 存储去重统计数据
 */
export interface VaultStatsDto {
  totalRecords: number;
  uniqueObjects: number;
  totalRawBytes: number;
  uniqueBytes: number;
  savedBytes: number;
  dedupRatioPercent: number;
  formattedTotalRaw: string;
  formattedSavedBytes: string;
}

/**
 * WebDAV 配置对象
 */
export interface WebdavConfigDto {
  url: string;
  username: string;
  password?: string;
  vaultId: string;
}

/**
 * WebDAV 远端能力探测结果
 */
export interface WebdavCapabilityDto {
  reachable: boolean;
  serverHeader?: string | null;
  davCompliance: string[];
  supportsLock: boolean;
  message: string;
}

/**
 * 归档执行报告
 */
export interface ArchiveResultDto {
  uploadedCount: number;
  verifiedCount: number;
  failedCount: number;
  durationMs: number;
}

/**
 * 应用设置（Vault/设备/定时/采集目录）
 */
export interface AppSettingsDto {
  vaultId: string;
  deviceId: string;
  webdavUrl: string;
  webdavUsername: string;
  copyThresholdMib: number;
  cacheRetentionDays: number;
  cacheMaxMib: number;
  scanIntervalMinutes: number;
  scheduleEnabled: boolean;
  collectDirs: string[];
}

/**
 * 上传任务列表项
 */
export interface UploadTaskDto {
  taskId: string;
  status: string;
  retryCount: number;
  errorMessage?: string | null;
  updatedAt: string;
  originalName: string;
  recordId: string;
  hash: string;
  size: number;
  formattedSize: string;
  originalPath?: string | null;
}

/**
 * 同步操作结果
 */
export interface SyncResultDto {
  publishedSeq?: number | null;
  appliedEvents: number;
  totalRecords: number;
  totalObjects: number;
  message: string;
}
