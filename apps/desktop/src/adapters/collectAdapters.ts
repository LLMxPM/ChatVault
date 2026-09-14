// ChatVault 采集适配器前端注册表
// 新增来源类型时在此登记展示元数据与探测/识别入口，任务页与 composable 共用。

import {
  detectWechatAccounts,
  detectWxworkAccounts,
  inspectWechatDirectory,
  inspectWxworkDirectory,
  checkDirectory,
} from "../api/tauri";
import type { CollectSourceType, WechatAccountDto } from "../types";

/** 采集适配器定义：驱动任务页添加菜单、卡片文案与探测逻辑。 */
export interface CollectAdapterDef {
  id: CollectSourceType;
  /** 完整展示名 */
  label: string;
  /** 徽章短名 */
  badge: string;
  /** 徽章色调 */
  badgeTone: "accent" | "neutral";
  /** account：需选账号；folder：整目录递归 */
  kind: "account" | "folder";
  /** 是否提供视频识别开关 */
  supportsVideos: boolean;
  /** 视频开关旁的说明文案 */
  videoHint?: string;
  /** 空态/帮助中的根目录提示 */
  rootHint?: string;
  /** 目录有效但无账号时的提示 */
  emptyAccountDetail?: string;
  /** 目录型来源的说明 */
  folderDetail?: string;
  /** 检查根目录并返回账号；folder 型返回空数组 */
  inspect: (path: string) => Promise<WechatAccountDto[]>;
  /** 自动发现本机根目录；无能力的适配器省略 */
  discover?: () => Promise<WechatAccountDto[]>;
}

/** 当前已注册的采集适配器。 */
export const COLLECT_ADAPTERS: CollectAdapterDef[] = [
  {
    id: "wechat-windows-4",
    label: "微信 4.x",
    badge: "微信 4.x",
    badgeTone: "accent",
    kind: "account",
    supportsVideos: true,
    videoHint: "扫描 msg/video 下的 .mp4",
    rootHint: "xwechat_files",
    emptyAccountDetail: "目录有效，但暂未发现微信账号。",
    inspect: inspectWechatDirectory,
    discover: detectWechatAccounts,
  },
  {
    id: "wxwork-windows",
    label: "企业微信",
    badge: "企业微信",
    badgeTone: "accent",
    kind: "account",
    supportsVideos: true,
    videoHint: "扫描 Cache/Video 下的 .mp4",
    rootHint: "WXWork",
    emptyAccountDetail: "目录有效，但暂未发现企业微信账号。",
    inspect: inspectWxworkDirectory,
    discover: detectWxworkAccounts,
  },
  {
    id: "generic-folder",
    label: "附件目录",
    badge: "附件",
    badgeTone: "neutral",
    kind: "folder",
    supportsVideos: false,
    folderDetail: "递归扫描该目录中的附件文件。",
    inspect: async () => [],
  },
];

const byId = new Map(COLLECT_ADAPTERS.map((a) => [a.id, a]));

/** 按类型取适配器定义；未知类型返回 undefined。 */
export function getCollectAdapter(
  sourceType: CollectSourceType | string,
): CollectAdapterDef | undefined {
  return byId.get(sourceType as CollectSourceType);
}

/** 是否账号型来源。 */
export function isAccountAdapter(sourceType: CollectSourceType | string): boolean {
  return getCollectAdapter(sourceType)?.kind === "account";
}

/** 支持自动发现的适配器列表（添加菜单分组用）。 */
export function discoverableCollectAdapters(): CollectAdapterDef[] {
  return COLLECT_ADAPTERS.filter((a) => a.discover);
}

/** 校验目录是否可作为通用附件目录。 */
export async function validateFolderSource(path: string): Promise<boolean> {
  return checkDirectory(path);
}
