<!-- 拾文应用信息：展示版本、数据位置与日志入口。 -->
<template>
  <UiCard title="关于拾文 ChatVault">
    <div class="space-y-1.5 text-cv-caption text-cv-text-2">
      <p>版本：{{ runtime.version || "桌面端连接中" }}</p>
      <p class="break-all">数据目录：{{ runtime.dataDirectory || "请在桌面应用中查看" }}</p>
      <p>关闭窗口将退出应用；已启用的系统定时任务仍会独立执行。卸载默认保留资料库与系统凭据。</p>
      <UiButton
        size="sm"
        variant="ghost"
        :disabled="!runtime.ready"
        @click="showLogs"
      >
        打开日志目录
      </UiButton>
      <p v-if="error" class="text-cv-danger">{{ error }}</p>
    </div>
  </UiCard>
</template>
<script setup lang="ts">
import { ref } from "vue";
import UiCard from "./ui/UiCard.vue";
import UiButton from "./ui/UiButton.vue";
import { runtime, openLogDirectory } from "../api/runtime";
import { pushToast } from "../composables/useToast";

const error = ref("");

/** 打开日志目录。 */
async function showLogs() {
  try {
    await openLogDirectory();
    error.value = "";
  } catch (reason) {
    error.value = String(reason);
    pushToast({ tone: "danger", title: "无法打开日志目录", description: String(reason) });
  }
}
</script>
