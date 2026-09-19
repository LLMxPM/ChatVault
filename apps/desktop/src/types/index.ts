// ChatVault 桌面端通用数据类型定义
// 用于前端与 Rust Tauri 后端通过 invoke 进行交互的契约对象

/** 采集源适配器类型。 */
export type CollectSourceType = "wechat-windows-4" | "wxwork-windows" | "generic-folder";

/** 持久化采集源配置。 */
export interface CollectSourceDto {
  sourceType: CollectSourceType;
  path: string;
  /** 账号型来源（微信/企业微信）是否识别视频；非账号来源忽略。默认开启。 */
  enableVideos?: boolean;
}

/** 采集源探测快照状态。 */
export type CollectSourceStatus =
  | "checking"
  | "ready"
  | "empty"
  | "missing"
  | "error";

/** 采集源上次探测快照：任务页首屏直接渲染。 */
export interface CollectSourceCacheDto {
  path: string;
  status: CollectSourceStatus;
  errorMessage: string;
  accounts: WechatAccountDto[];
  inspectedAt: number;
}

/** 微信 4.x 账号探测信息。 */
export interface WechatAccountDto {
  sourceAccountId: string;
  sourceDir: string;
  sourceRoot: string;
  /** 文件附件候选数（msg/file） */
  filesCountEstimated: number;
  /** 视频候选数（msg/video 下的 .mp4） */
  videosCountEstimated: number;
}

/** 立即扫描时用于定位微信账号的目录与账号标识。 */
export interface WechatAccountTargetDto {
  sourceRoot: string;
  sourceAccountId: string;
}

/**
 * 扫描请求参数
 */
export interface ScanRequestDto {
  targetAccounts: WechatAccountTargetDto[];
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

/** 内容对象位置状态。 */
export type FileLocation = "local" | "remote" | "both" | "missing";

/** 按内容折叠后的文件库列表项。 */
export interface FileObjectViewDto {
  objectId: string;
  hash: string;
  originalName: string;
  extension: string;
  fileSize: number;
  formattedSize: string;
  category: "doc" | "image" | "video" | "audio" | "archive" | "other";
  fileTime?: string | null;
  discoveredAt?: string | null;
  timeSource?: string | null;
  location: FileLocation;
  openPath?: string | null;
  sourceCount: number;
}

/** 对象检索分页信封。 */
export interface ObjectSearchPageDto {
  total: number;
  items: FileObjectViewDto[];
}

/** 批量操作单项结果。 */
export interface BatchItemResultDto {
  objectId: string;
  originalName: string;
  status: "ok" | "failed" | "partial" | "skipped";
  savedPath?: string | null;
  releasedBytes?: number | null;
  error?: string | null;
}

/** 批量操作汇总。 */
export interface BatchResultDto {
  total: number;
  okCount: number;
  failedCount: number;
  releasedBytes: number;
  items: BatchItemResultDto[];
}

/** 内容对象展开后的来源条目。 */
export interface FileSourceDto {
  recordId: string;
  originalName: string;
  originalPath: string;
  sourceType: string;
  sourceAccountId?: string | null;
  sourceAccountName?: string | null;
  sourceConversationId?: string | null;
  sourceConversationName?: string | null;
  fileTime?: string | null;
  discoveredAt: string;
  deviceId: string;
  /** 设备展示名称 */
  deviceName?: string | null;
  /** 是否本机设备 */
  isLocal: boolean;
  hasLocalPath: boolean;
}

/** 远端对象下载结果。 */
export interface DownloadResultDto {
  savedPath: string;
  fileName: string;
  size: number;
  /** 下载目录已存在同内容副本时为 true，未发生新的网络下载/复制 */
  skipped: boolean;
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
  startTime?: string;
  endTime?: string;
  timeField?: "file_time" | "discovered_at";
  location?: FileLocation | "";
  /** 仅返回已隐藏对象；默认 false */
  hidden?: boolean;
  extensions?: string[];
  sort?: string;
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
 * 应用设置（Vault/设备/WebDAV/缓存/采集源/调度）
 */
export interface AppSettingsDto {
  /** 完整 Vault ID，格式为 chatvault-xxxx */
  vaultId: string;
  /** 系统生成的设备 ID，不可修改 */
  deviceId: string;
  /** 可配置设备名称 */
  deviceName: string;
  webdavUrl: string;
  webdavUsername: string;
  /** 已关联网盘的 Vault ID；未关联时为 null */
  boundVaultId: string | null;
  /** 已关联网盘的存储地址；未关联时为 null */
  boundWebdavUrl: string | null;
  copyThresholdMib: number;
  cacheRetentionDays: number;
  cacheMaxMib: number;
  downloadDir: string;
  scanIntervalMinutes: number;
  scheduleEnabled: boolean;
  collectSources: CollectSourceDto[];
}

/** 清空旧 Vault 绑定后的清理摘要 */
export interface VaultResetReportDto {
  hadBinding: boolean;
  clearedCursors: number;
  clearedJournalEvents: number;
  clearedAppliedEvents: number;
  clearedDevices: number;
  requeuedUploads: number;
  clearedSettings: number;
}

/** 流水线请求：与定时任务同构。 */
export interface PipelineRequestDto {
  /** 未传或 null 表示扫描全部微信账号。数组为空表示跳过微信。 */
  targetAccounts?: WechatAccountTargetDto[] | null;
  fullScan?: boolean;
}

/** 流水线结果。 */
export interface PipelineResultDto {
  runId: string;
  webdavConfigured: boolean;
  scan: ScanResultDto;
  archive: ArchiveResultDto | null;
  syncMessage: string | null;
  message: string;
  durationMs: number;
  status: string;
}

/**
 * 上传队列项（upload_tasks，跨运行单文件待上传/已校验记录）
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

/** 运行列表项（task_runs，一次手动/定时执行） */
export interface TaskRunDto {
  runId: string;
  kind: string;
  triggerSource: string;
  status: string;
  startedAt: string;
  finishedAt?: string | null;
  durationMs?: number | null;
  webdavConfigured: boolean;
  summaryJson?: string | null;
  errorMessage?: string | null;
  failedItems: number;
  /** 执行方：desktop | cli */
  runnerKind?: string;
  /** 是否已请求取消 */
  cancelRequested?: boolean;
}

/** 运行阶段（task_run_stages：scan/archive/publish/pull） */
export interface TaskRunStageDto {
  stage: string;
  status: string;
  startedAt?: string | null;
  finishedAt?: string | null;
  durationMs?: number | null;
  statsJson?: string | null;
  message?: string | null;
}

/** 运行关键明细（task_run_items，仅异常文件） */
export interface TaskRunItemDto {
  itemId: string;
  stage: string;
  recordId?: string | null;
  taskId?: string | null;
  name: string;
  status: string;
  errorCode?: string | null;
  errorMessage?: string | null;
  size?: number | null;
  updatedAt: string;
}

/** 运行详情 */
export interface TaskRunDetailDto {
  run: TaskRunDto;
  stages: TaskRunStageDto[];
  items: TaskRunItemDto[];
}

/** 运行中的实时阶段信息（来自 DB 轮询或 Tauri 事件） */
export interface ActiveRunDto {
  runId: string;
  kind: string;
  triggerSource: string;
  runnerKind: string;
  status: string;
  startedAt: string;
  heartbeatAt?: string | null;
  cancelRequested: boolean;
  currentStage?: string | null;
  stageStatus?: string | null;
  done?: number | null;
  total?: number | null;
  currentName?: string | null;
}

/** 运行进度事件载荷 */
export interface RunProgressEvent {
  runId: string;
  stage: string;
  done: number;
  total: number;
  currentName?: string | null;
}

/** 运行阶段事件载荷 */
export interface RunStageEvent {
  runId: string;
  stage: string;
  status: string;
  message?: string | null;
}

/** 运行关键项事件载荷 */
export interface RunItemEvent {
  runId: string;
  stage: string;
  taskId?: string | null;
  recordId?: string | null;
  name: string;
  status: string;
  error?: string | null;
  size?: number | null;
}

/** 运行结束事件载荷 */
export interface RunFinishedEvent {
  runId: string;
  status: string;
  message?: string | null;
}
