<!-- 拾文应用信息：版本、数据位置、数据目录与开源仓库入口。 -->
<template>
  <UiCard
    title="关于拾文 ChatVault"
    info="关闭窗口将退出应用；已启用的系统定时任务仍会独立执行。卸载默认保留资料库与系统凭据。"
  >
    <div class="space-y-2 text-cv-caption text-cv-text-2">
      <p>版本：{{ runtime.version || "桌面端连接中" }}</p>
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
      <p v-if="error" class="text-cv-danger">{{ error }}</p>
    </div>
  </UiCard>
</template>
<script setup lang="ts">
import { ref } from "vue";
import UiCard from "./ui/UiCard.vue";
import UiButton from "./ui/UiButton.vue";
import { runtime, openDataDirectory, openRepositoryHomepage } from "../api/runtime";
import { pushToast } from "../composables/useToast";

const repositoryUrl = "https://github.com/LLMxPM/ChatVault";
const error = ref("");

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
</script>
