// 全局「待完成配置」状态：采集源与 WebDAV 是否已配置。
// 设置/任务页读写同一份状态，避免 keep-alive 下任务页徽标过期。

import { computed, reactive, ref } from "vue";
import { getAppSettings } from "../api/tauri";
import type { AppTab } from "./useNav";

interface SetupStatusState {
  loaded: boolean;
  collectSourcesConfigured: boolean;
  webdavConfigured: boolean;
}

const state = reactive<SetupStatusState>({
  loaded: false,
  collectSourcesConfigured: false,
  webdavConfigured: false,
});

/** 已保存的 WebDAV 连接信息，供恢复索引等命令直接使用。 */
export const webdavConnection = ref({
  url: "",
  username: "",
  vaultId: "",
});

export interface SetupHint {
  id: string;
  label: string;
  tab: AppTab;
}

/** WebDAV 是否已配置（地址非空）。 */
export const isWebdavConfigured = computed(() => state.webdavConfigured);

/** 采集源是否已配置（列表非空）。 */
export const isCollectSourcesConfigured = computed(() => state.collectSourcesConfigured);

/** 未完成的配置项列表；已全部配置时为空。 */
export const setupHints = computed<SetupHint[]>(() => {
  if (!state.loaded) return [];
  const hints: SetupHint[] = [];
  if (!state.collectSourcesConfigured) {
    hints.push({ id: "collect", label: "未配置采集源", tab: "tasks" });
  }
  if (!state.webdavConfigured) {
    hints.push({ id: "webdav", label: "未连接 WebDAV", tab: "settings" });
  }
  return hints;
});

/** 从后端刷新配置状态。失败时保留上次结果。 */
export async function refreshSetupStatus(): Promise<void> {
  try {
    const settings = await getAppSettings();
    state.collectSourcesConfigured = (settings.collectSources?.length ?? 0) > 0;
    // 与任务页「WebDAV 已连接」判定保持一致：地址非空即视为已配置。
    state.webdavConfigured = Boolean(settings.webdavUrl?.trim());
    if (settings.webdavUrl?.trim()) {
      webdavConnection.value = {
        url: settings.webdavUrl,
        username: settings.webdavUsername,
        vaultId: settings.vaultId,
      };
    } else {
      webdavConnection.value = { url: "", username: "", vaultId: settings.vaultId || "" };
    }
    state.loaded = true;
  } catch {
    // 预览环境或短暂失败时不清空已知状态
  }
}

/** 本地采集源变更后立即同步角标，避免等待下次整页刷新。 */
export function markCollectSourcesConfigured(configured: boolean): void {
  state.collectSourcesConfigured = configured;
  state.loaded = true;
}

/** WebDAV 配置变更后立即同步徽标与连接信息。 */
export function markWebdavConfigured(
  configured: boolean,
  connection?: { url: string; username: string; vaultId: string },
): void {
  state.webdavConfigured = configured;
  state.loaded = true;
  if (connection) {
    webdavConnection.value = {
      url: configured ? connection.url.trim() : "",
      username: connection.username,
      vaultId: connection.vaultId,
    };
  } else if (!configured) {
    webdavConnection.value = {
      url: "",
      username: webdavConnection.value.username,
      vaultId: webdavConnection.value.vaultId,
    };
  }
}
