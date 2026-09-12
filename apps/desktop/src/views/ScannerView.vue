<!--
  ChatVault 微信 4.x 数据源与扫描视图
  职责：自动探测本机微信 4.x 账号与附件目录、发起增量去重扫描并展示执行报告。
-->
<template>
  <div class="h-full flex flex-col p-6 space-y-6 overflow-y-auto">
    <!-- 顶部标题与说明 -->
    <div class="border-b border-slate-800 pb-4">
      <h2 class="text-xl font-bold text-white flex items-center space-x-2">
        <span>微信 4.x 来源探测与增量扫描</span>
      </h2>
      <p class="text-xs text-slate-400 mt-1">
        专精 Windows 微信 4.x 架构（xwechat_files 目录），原生无加密原名提取，流式 BLAKE3 哈希入库去重。
      </p>
    </div>

    <!-- 微信账号探测面板 -->
    <div class="bg-slate-900/70 border border-slate-800 rounded-xl p-5 space-y-4">
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-semibold text-slate-200 flex items-center space-x-2">
          <span>本机探测到的微信 4.x 账号</span>
          <span class="text-xs px-2 py-0.5 rounded-full bg-emerald-950 text-emerald-400 border border-emerald-800/80">
            {{ accounts.length }} 个账号
          </span>
        </h3>
        <button
          class="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 transition-colors"
          @click="loadAccounts"
        >
          <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': detecting }" />
          <span>重新检测</span>
        </button>
      </div>

      <!-- 账号卡片列表 -->
      <div v-if="accounts.length > 0" class="grid grid-cols-1 md:grid-cols-2 gap-3">
        <div
          v-for="acc in accounts"
          :key="acc.sourceAccountId"
          class="p-4 rounded-lg border transition-all cursor-pointer flex items-start space-x-3"
          :class="
            selectedAccounts.includes(acc.sourceAccountId)
              ? 'bg-emerald-950/20 border-emerald-600/70'
              : 'bg-slate-950/50 border-slate-800/80 hover:border-slate-700'
          "
          @click="toggleAccount(acc.sourceAccountId)"
        >
          <input
            type="checkbox"
            :checked="selectedAccounts.includes(acc.sourceAccountId)"
            class="mt-1 rounded text-emerald-500 focus:ring-emerald-500 border-slate-700 bg-slate-800 cursor-pointer"
            @click.stop="toggleAccount(acc.sourceAccountId)"
          />
          <div class="flex-1 min-w-0">
            <div class="flex items-center justify-between">
              <p class="font-mono text-sm font-semibold text-emerald-400 truncate">
                {{ acc.sourceAccountId }}
              </p>
              <span class="text-[11px] text-slate-400">
                约 {{ acc.filesCountEstimated }} 个待扫文件
              </span>
            </div>
            <p class="text-xs text-slate-400 font-mono truncate mt-1" :title="acc.sourceDir">
              {{ acc.sourceDir }}
            </p>
          </div>
        </div>
      </div>

      <div v-else class="text-center py-8 text-slate-500 text-xs">
        未在系统文档目录下探测到微信 4.x 目录（Documents\xwechat_files）。
      </div>
    </div>

    <!-- 通用目录补充 -->
    <div class="bg-slate-900/70 border border-slate-800 rounded-xl p-5 space-y-3">
      <h3 class="text-sm font-semibold text-slate-200">
        通用本地文件夹（可选）
      </h3>
      <div class="flex items-center space-x-2">
        <input
          v-model="customFolderPath"
          type="text"
          placeholder="输入或粘贴自定义本地文件夹路径（例如 C:\Users\Downloads\Work）..."
          class="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-emerald-500"
        />
        <button
          class="px-3 py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium"
          @click="addCustomFolder"
        >
          添加路径
        </button>
      </div>

      <div v-if="customFolders.length > 0" class="space-y-1">
        <div
          v-for="(path, idx) in customFolders"
          :key="path"
          class="flex items-center justify-between text-xs font-mono bg-slate-950 px-3 py-1.5 rounded border border-slate-800 text-slate-300"
        >
          <span class="truncate">{{ path }}</span>
          <button class="text-red-400 hover:text-red-300 ml-2" @click="customFolders.splice(idx, 1)">
            移除
          </button>
        </div>
      </div>
    </div>

    <!-- 扫描操作与执行按钮 -->
    <div class="pt-2 space-y-2">
      <label class="flex items-center space-x-2 text-xs text-slate-300">
        <input
          v-model="fullScan"
          type="checkbox"
          class="rounded text-emerald-500 focus:ring-emerald-500 border-slate-700 bg-slate-800 cursor-pointer"
        />
        <span>强制全量扫描（忽略增量检查点，用于首次或目录异常后校准）</span>
      </label>
      <button
        class="w-full py-3 px-4 rounded-xl font-semibold text-sm transition-all flex items-center justify-center space-x-2 shadow-lg"
        :class="
          scanning || (selectedAccounts.length === 0 && customFolders.length === 0)
            ? 'bg-slate-800 text-slate-500 cursor-not-allowed'
            : 'bg-emerald-600 hover:bg-emerald-500 text-white shadow-emerald-900/30'
        "
        :disabled="scanning || (selectedAccounts.length === 0 && customFolders.length === 0)"
        @click="startScanning"
      >
        <Play v-if="!scanning" class="w-4 h-4 fill-current" />
        <RefreshCw v-else class="w-4 h-4 animate-spin" />
        <span>
          {{
            scanning
              ? "正在进行哈希与入库去重..."
              : fullScan
                ? "开始全量扫描与入库"
                : "开始增量扫描与入库"
          }}
        </span>
      </button>
      <p class="text-[11px] text-slate-500">
        默认增量：只进入 mtime 晚于上次扫描的目录，并复检已索引文件的内容变更；未变更文件不读内容。
      </p>
    </div>

    <!-- 扫描结果报告卡片 -->
    <div
      v-if="scanResult"
      class="bg-emerald-950/20 border border-emerald-800/60 rounded-xl p-5 space-y-3"
    >
      <div class="flex items-center justify-between">
        <h4 class="text-sm font-semibold text-emerald-400 flex items-center space-x-1.5">
          <CheckCircle2 class="w-4 h-4 text-emerald-400" />
          <span>本次扫描任务完成</span>
        </h4>
        <span class="text-xs text-slate-400">总耗时：{{ scanResult.durationMs }} ms</span>
      </div>

      <div class="grid grid-cols-3 gap-3 pt-2">
        <div class="bg-slate-900/80 p-3 rounded-lg border border-slate-800">
          <p class="text-[11px] text-slate-400">发现文件总数</p>
          <p class="text-lg font-bold text-white mt-0.5">{{ scanResult.totalDiscovered }}</p>
        </div>
        <div class="bg-slate-900/80 p-3 rounded-lg border border-slate-800">
          <p class="text-[11px] text-slate-400">已存在对象（去重跳过）</p>
          <p class="text-lg font-bold text-emerald-400 mt-0.5">{{ scanResult.totalSkipped }}</p>
        </div>
        <div class="bg-slate-900/80 p-3 rounded-lg border border-slate-800">
          <p class="text-[11px] text-slate-400">新增内容对象 (Object)</p>
          <p class="text-lg font-bold text-blue-400 mt-0.5">{{ scanResult.totalNewObjects }}</p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { RefreshCw, Play, CheckCircle2 } from "lucide-vue-next";
import { detectWechatAccounts, runScan } from "../api/tauri";
import type { WechatAccountDto, ScanResultDto } from "../types";

const accounts = ref<WechatAccountDto[]>([]);
const selectedAccounts = ref<string[]>([]);
const customFolderPath = ref("");
const customFolders = ref<string[]>([]);

const detecting = ref(false);
const scanning = ref(false);
const fullScan = ref(false);
const scanResult = ref<ScanResultDto | null>(null);

/**
 * 探测本机微信 4.x 账号
 */
async function loadAccounts() {
  detecting.value = true;
  try {
    const list = await detectWechatAccounts();
    accounts.value = list;
    // 默认全选检测到的账号
    selectedAccounts.value = list.map((a) => a.sourceAccountId);
  } catch (err) {
    console.error("探测微信账号失败:", err);
  } finally {
    detecting.value = false;
  }
}

/**
 * 切换账号勾选
 */
function toggleAccount(accId: string) {
  const idx = selectedAccounts.value.indexOf(accId);
  if (idx >= 0) {
    selectedAccounts.value.splice(idx, 1);
  } else {
    selectedAccounts.value.push(accId);
  }
}

/**
 * 添加通用自定义目录
 */
function addCustomFolder() {
  const p = customFolderPath.value.trim();
  if (p && !customFolders.value.includes(p)) {
    customFolders.value.push(p);
    customFolderPath.value = "";
  }
}

/**
 * 触发增量扫描
 */
async function startScanning() {
  scanning.value = true;
  scanResult.value = null;
  try {
    const res = await runScan({
      targetAccounts: selectedAccounts.value,
      customFolders: customFolders.value,
      fullScan: fullScan.value,
    });
    scanResult.value = res;
  } catch (err) {
    alert("扫描失败: " + err);
  } finally {
    scanning.value = false;
  }
}

onMounted(() => {
  loadAccounts();
});
</script>
