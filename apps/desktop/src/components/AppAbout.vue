<!-- 拾文应用信息：展示版本、数据位置、退出行为和日志入口。 -->
<template>
  <section class="bg-slate-900/70 border border-slate-800 rounded-xl p-5 space-y-3 max-w-2xl text-xs">
    <h3 class="text-sm font-semibold text-slate-200">关于拾文 ChatVault</h3>
    <p class="text-slate-400">版本：{{ runtime.version || "桌面端连接中" }}</p>
    <p class="text-slate-400 break-all">数据目录：{{ runtime.dataDirectory || "请在桌面应用中查看" }}</p>
    <p class="text-slate-400">关闭窗口将退出应用；已启用的系统定时任务仍会独立执行。卸载默认保留资料库、附件副本和系统凭据。</p>
    <button class="text-emerald-400 hover:text-emerald-300 disabled:opacity-50" :disabled="!runtime.ready" @click="showLogs">打开日志目录</button>
    <p v-if="error" role="alert" class="text-red-400">{{ error }}</p>
  </section>
</template>
<script setup lang="ts">
import { ref } from "vue";
import { runtime, openLogDirectory } from "../api/runtime";
const error = ref("");
/** 打开日志目录并展示系统调用失败原因。 */
async function showLogs() {
  try {
    await openLogDirectory();
    error.value = "";
  } catch (reason) {
    error.value = String(reason);
  }
}
</script>
