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
