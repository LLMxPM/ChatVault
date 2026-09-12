// ChatVault 桌面端通用数据类型定义
// 用于前端与 Rust Tauri 后端通过 invoke 进行交互的契约对象

/**
 * 微信 4.x 账号探测信息
 */
export interface WechatAccountDto {
  sourceAccountId: string;
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
  sourceAccountId?: string | null;
  sourceAccountName?: string | null;
  sourceConversationId?: string | null;
  sourceConversationName?: string | null;
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
  sourceType?: string;
  sourceAccountId?: string;
  sourceConversationId?: string;
  limit?: number;
  offset?: number;
}

/** 来源账号映射。 */
export interface SourceAccountDto {
  sourceType: string;
  sourceAccountId: string;
  sourceName?: string | null;
  displayName?: string | null;
  effectiveName: string;
  isFavorite: boolean;
  recordCount: number;
}

/** 来源聊天映射。 */
export interface SourceConversationDto {
  sourceType: string;
  sourceAccountId: string;
  sourceConversationId: string;
  sourceName?: string | null;
  displayName?: string | null;
  effectiveName: string;
  isFavorite: boolean;
  recordCount: number;
}

/** 来源账号名称/收藏更新请求。 */
export interface UpdateSourceAccountDto {
  sourceType: string;
  sourceAccountId: string;
  displayName?: string | null;
  isFavorite: boolean;
}

/** 来源聊天名称/收藏更新请求。 */
export interface UpdateSourceConversationDto {
  sourceType: string;
  sourceAccountId: string;
  sourceConversationId: string;
  displayName?: string | null;
  isFavorite: boolean;
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
  authenticated: boolean;
  supportMkcol: boolean;
  supportMove: boolean;
  message: string;
  durationMs: number;
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
 * 应用设置（Vault/设备/WebDAV/缓存/采集目录/调度）
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

/** 流水线请求：与定时任务同构。 */
export interface PipelineRequestDto {
  /** 未传或 null 表示扫描全部微信账号 */
  targetAccounts?: string[] | null;
  /** 仅本次附加的目录，不写入设置 */
  extraFolders?: string[];
  fullScan?: boolean;
}

/** 流水线结果。 */
export interface PipelineResultDto {
  webdavConfigured: boolean;
  scan: ScanResultDto;
  archive: ArchiveResultDto | null;
  syncMessage: string | null;
  message: string;
  durationMs: number;
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
