<!-- 拾文应用信息：版本、升级检测、数据位置、数据目录、开源许可与仓库入口。 -->
<template>
  <UiCard
    title="关于拾文 ChatVault"
    info="关闭窗口将退出应用；已启用的系统定时任务仍会独立执行。卸载默认保留资料库与系统凭据。升级仅检测 GitHub 正式版 Release，不含预发布。"
  >
    <div class="space-y-2 text-cv-caption text-cv-text-2">
      <div class="flex min-w-0 flex-wrap items-center gap-2">
        <span class="min-w-0 flex-1">
          版本：{{ runtime.version || "桌面端连接中" }}
          <span v-if="updateInfo" class="ml-2">
            最新正式版：<span class="text-cv-accent">v{{ updateInfo.latestVersion }}</span>
          </span>
        </span>
        <UiButton
          size="sm"
          variant="secondary"
          class="shrink-0"
          :disabled="!runtime.ready"
          :loading="checking"
          @click="runCheck()"
        >
          检查更新
        </UiButton>
        <UiButton
          v-if="updateInfo?.hasUpdate"
          size="sm"
          variant="primary"
          class="shrink-0"
          :disabled="!runtime.ready"
          :loading="installing"
          @click="runInstall"
        >
          下载并安装
        </UiButton>
        <UiButton
          v-if="updateInfo && !updateInfo.hasUpdate"
          size="sm"
          variant="secondary"
          class="shrink-0"
          @click="openReleases"
        >
          发布页
        </UiButton>
      </div>
      <p v-if="updateStatus" class="text-cv-caption">{{ updateStatus }}</p>
      <div class="flex min-w-0 flex-wrap items-center gap-2">
        <span class="min-w-0 flex-1 break-all">
          数据目录：{{ runtime.dataDirectory || "请在桌面应用中查看" }}
        </span>
        <UiButton
          size="sm"
          variant="secondary"
          class="shrink-0"
          :disabled="!runtime.ready"
          @click="showDataDir"
        >
          打开数据目录
        </UiButton>
      </div>
      <div class="flex min-w-0 flex-wrap items-center gap-2">
        <span class="min-w-0 flex-1 break-all">
          项目仓库：<span class="text-cv-accent">{{ repositoryUrl }}</span>
        </span>
        <UiButton size="sm" variant="secondary" class="shrink-0" @click="openRepository">
          打开仓库 Star
        </UiButton>
      </div>
      <p>
        开源许可：<span class="text-cv-accent">AGPL-3.0-or-later</span>
        · 完整条款见仓库 LICENSE 文件
      </p>
      <p v-if="error" class="text-cv-danger">{{ error }}</p>
    </div>
  </UiCard>
</template>
<script setup lang="ts">
import { onMounted, ref } from "vue";
import UiCard from "./ui/UiCard.vue";
import UiButton from "./ui/UiButton.vue";
import {
  runtime,
  openDataDirectory,
  openRepositoryHomepage,
  checkForUpdate,
  downloadAndInstallUpdate,
  openReleasePage,
  type UpdateCheckResult,
} from "../api/runtime";
import { pushToast } from "../composables/useToast";

const repositoryUrl = "https://github.com/LLMxPM/ChatVault";
const error = ref("");
const checking = ref(false);
const installing = ref(false);
const updateInfo = ref<UpdateCheckResult | null>(null);
const updateStatus = ref("");

/** 打开应用数据目录。 */
async function showDataDir() {
  try {
    await openDataDirectory();
    error.value = "";
  } catch (reason) {
    error.value = String(reason);
    pushToast({ tone: "danger", title: "无法打开数据目录", description: String(reason) });
  }
}

/** 用系统默认浏览器打开项目仓库。 */
async function openRepository() {
  try {
    await openRepositoryHomepage();
    error.value = "";
  } catch (reason) {
    error.value = String(reason);
    pushToast({ tone: "danger", title: "无法打开项目仓库", description: String(reason) });
  }
}

/** 打开 GitHub 正式版发布列表。 */
async function openReleases() {
  try {
    await openReleasePage();
  } catch (reason) {
    pushToast({ tone: "danger", title: "无法打开发布页", description: String(reason) });
  }
}

/** 检查 GitHub 正式 Release；silent 时只更新状态文案，不弹 toast。 */
async function runCheck(options: { silent?: boolean } = {}) {
  const silent = options.silent === true;
  checking.value = true;
  if (!silent) {
    error.value = "";
    updateStatus.value = "正在检查更新…";
  }
  try {
    const result = await checkForUpdate();
    updateInfo.value = result;
    if (result.hasUpdate) {
      updateStatus.value = `发现新版本 v${result.latestVersion}，可下载安装。`;
      if (!silent) {
        pushToast({
          tone: "success",
          title: `发现新版本 v${result.latestVersion}`,
          description: result.installerName
            ? `安装包：${result.installerName}`
            : "请下载并安装正式版。",
        });
      }
    } else {
      updateStatus.value = silent ? "" : "当前已是最新正式版。";
      if (!silent) {
        pushToast({ tone: "info", title: "已是最新正式版" });
      }
    }
  } catch (reason) {
    updateInfo.value = null;
    if (!silent) {
      updateStatus.value = "";
      error.value = String(reason);
      pushToast({ tone: "danger", title: "检查更新失败", description: String(reason) });
    } else {
      // 静默失败不打断用户；可手动再检查。
      updateStatus.value = "";
    }
  } finally {
    checking.value = false;
  }
}

/** 下载并启动安装最新正式版。 */
async function runInstall() {
  installing.value = true;
  error.value = "";
  updateStatus.value = "正在下载安装包并校验…";
  try {
    await downloadAndInstallUpdate();
    updateStatus.value = "已启动安装程序，请按提示完成安装。";
    pushToast({
      tone: "success",
      title: "安装程序已启动",
      description: "请按安装向导完成升级，完成后重新打开应用。",
      durationMs: 6000,
    });
  } catch (reason) {
    error.value = String(reason);
    updateStatus.value = "";
    pushToast({ tone: "danger", title: "更新失败", description: String(reason) });
  } finally {
    installing.value = false;
  }
}

onMounted(() => {
  // 桌面就绪后静默检查一次；失败不打扰，保留手动检查入口。
  if (runtime.ready) {
    void runCheck({ silent: true });
  }
});
</script>
