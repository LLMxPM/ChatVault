<!--
  ChatVault 存储统计与去重看板视图
  职责：展示当前文件库的附件记录数、内容对象数、原始占用空间及 BLAKE3 全局去重节省指标。
-->
<template>
  <div class="h-full flex flex-col p-6 space-y-6 overflow-y-auto">
    <!-- 顶部标题 -->
    <div class="border-b border-slate-800 pb-4 flex items-center justify-between">
      <div>
        <h2 class="text-xl font-bold text-white">存储看板与去重指标</h2>
        <p class="text-xs text-slate-400 mt-1">
          ChatVault 采用 BLAKE3 内容寻址架构，跨微信账号、跨聊天会话实现全局内容去重。
        </p>
      </div>
      <button
        class="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 transition-colors"
        @click="loadStats"
      >
        <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': loading }" />
        <span>刷新数据</span>
      </button>
    </div>

    <!-- 统计卡片网格 -->
    <div v-if="stats" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
      <!-- 总附件记录 -->
      <div class="bg-slate-900/80 border border-slate-800 rounded-xl p-5 relative overflow-hidden">
        <div class="text-slate-400 text-xs font-medium">累计发现附件记录 (Records)</div>
        <div class="text-2xl font-black text-white mt-2">{{ stats.totalRecords }}</div>
        <div class="text-[11px] text-slate-500 mt-2">微信多会话、多路径来源</div>
      </div>

      <!-- 唯一内容对象 -->
      <div class="bg-slate-900/80 border border-slate-800 rounded-xl p-5 relative overflow-hidden">
        <div class="text-slate-400 text-xs font-medium">唯一内容对象 (Objects)</div>
        <div class="text-2xl font-black text-emerald-400 mt-2">{{ stats.uniqueObjects }}</div>
        <div class="text-[11px] text-slate-500 mt-2">BLAKE3 唯一哈希去重后实体</div>
      </div>

      <!-- 原始发现体量 -->
      <div class="bg-slate-900/80 border border-slate-800 rounded-xl p-5 relative overflow-hidden">
        <div class="text-slate-400 text-xs font-medium">原始文件总大小</div>
        <div class="text-2xl font-black text-blue-400 mt-2">{{ stats.formattedTotalRaw }}</div>
        <div class="text-[11px] text-slate-500 mt-2">微信本地占用累计估算</div>
      </div>

      <!-- 去重节省 -->
      <div class="bg-slate-900/80 border border-emerald-800/40 rounded-xl p-5 relative overflow-hidden bg-gradient-to-br from-slate-900 to-emerald-950/20">
        <div class="text-emerald-400 text-xs font-medium flex items-center justify-between">
          <span>去重节约体积</span>
          <span class="px-1.5 py-0.5 rounded bg-emerald-900/60 text-[10px] text-emerald-300 font-bold">
            节省 {{ stats.dedupRatioPercent }}%
          </span>
        </div>
        <div class="text-2xl font-black text-emerald-300 mt-2">{{ stats.formattedSavedBytes }}</div>
        <div class="text-[11px] text-emerald-500/80 mt-2">避免重复同步与空间冗余</div>
      </div>
    </div>

    <!-- 去重架构原理解析 -->
    <div class="bg-slate-900/50 border border-slate-800/80 rounded-xl p-6 space-y-3 text-xs text-slate-300 leading-relaxed">
      <h3 class="text-sm font-semibold text-white">关于 ChatVault 内容去重机制</h3>
      <p>
        1. <strong>多源归一</strong>：当同一份 PDF 或图片在多个群聊（或不同微信账号）被转发时，ChatVault 会提取多条包含原始路径与会话上下文的 <code class="text-emerald-400 bg-slate-950 px-1 py-0.5 rounded">file_records</code>，但底层仅引用一个唯一的 <code class="text-emerald-400 bg-slate-950 px-1 py-0.5 rounded">file_objects</code>。
      </p>
      <p>
        2. <strong>云端极简归档</strong>：推送到 WebDAV 时，同哈希文件只上传一次，大幅降低网盘流量消耗与跨设备备份时间。
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { RefreshCw } from "lucide-vue-next";
import { getVaultStats } from "../api/tauri";
import type { VaultStatsDto } from "../types";

const stats = ref<VaultStatsDto | null>(null);
const loading = ref(false);

/**
 * 读取存储去重统计信息
 */
async function loadStats() {
  loading.value = true;
  try {
    const res = await getVaultStats();
    stats.value = res;
  } catch (err) {
    console.error("读取统计失败:", err);
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  loadStats();
});
</script>
