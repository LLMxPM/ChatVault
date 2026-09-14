// 拾文运行信息接口与共享状态：读取真实版本、用户数据目录和首次使用状态。
import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";

export const runtime = reactive({
  version: "",
  dataDirectory: "",
  firstRun: false,
  ready: false,
  error: "",
});

/** 获取桌面初始化结果；浏览器预览没有原生接口时显示明确状态。 */
export async function loadRuntime(): Promise<void> {
  try {
    const info = await invoke<{ version: string; dataDirectory: string; firstRun: boolean }>("get_runtime_info");
    Object.assign(runtime, info, { ready: true, error: "" });
  } catch (error) {
    runtime.error = String(error);
  }
}

/** 打开应用数据目录。 */
export async function openDataDirectory(): Promise<void> {
  await invoke("open_data_directory");
}

/** 打开应用自己管理的日志目录。 */
export async function openLogDirectory(): Promise<void> {
  await invoke("open_log_directory");
}

/** 用系统默认浏览器打开项目开源仓库。 */
export async function openRepositoryHomepage(): Promise<void> {
  await invoke("open_repository_homepage");
}

/** GitHub 正式 Release 更新检测结果。 */
export interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  releaseUrl: string;
  releaseTitle: string;
  installerName: string | null;
  installerSize: number | null;
}

/** 检查 GitHub 最新正式 Release（不含预发布）。 */
export async function checkForUpdate(): Promise<UpdateCheckResult> {
  return invoke<UpdateCheckResult>("check_for_update");
}

/** 下载最新正式版安装包并启动安装。 */
export async function downloadAndInstallUpdate(): Promise<void> {
  await invoke("download_and_install_update");
}

/** 打开 GitHub Releases 页面。 */
export async function openReleasePage(): Promise<void> {
  await invoke("open_release_page");
}
