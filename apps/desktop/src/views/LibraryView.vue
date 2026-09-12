<!--
  ChatVault 文件库与全文检索视图
  职责：提供基于 SQLite FTS5 的中文即输即搜、按文件类型和来源账号的多维筛选、文件在 Windows Explorer 中的定位。
-->
<template>
  <div class="h-full flex flex-col p-6 space-y-4">
    <!-- 顶部检索与多维筛选栏 -->
    <div class="flex flex-col md:flex-row gap-3 items-stretch md:items-center justify-between">
      <!-- 搜索框 -->
      <div class="relative flex-1">
        <Search class="absolute left-3 top-2.5 w-4 h-4 text-slate-400" />
        <input
          v-model="keyword"
          type="text"
          placeholder="搜索文件名、关键词（支持中文单字、双字、拼音/英文）..."
          class="w-full bg-slate-900 border border-slate-800 rounded-lg pl-9 pr-4 py-2 text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:border-emerald-500 transition-colors"
          @input="onSearchInput"
        />
        <button
          v-if="keyword"
          class="absolute right-3 top-2.5 text-xs text-slate-400 hover:text-slate-200"
          @click="clearKeyword"
        >
          ✕
        </button>
      </div>

      <!-- 来源账号筛选 -->
      <div class="flex items-center space-x-2">
        <select
          v-model="selectedAccount"
          class="bg-slate-900 border border-slate-800 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500"
          @change="fetchRecords()"
        >
          <option value="">全部微信账号与来源</option>
          <option v-for="acc in accountList" :key="`${acc.sourceType}:${acc.sourceAccountId}`" :value="`${acc.sourceType}\t${acc.sourceAccountId}`">
            {{ acc.effectiveName }}（{{ acc.sourceType }}）
          </option>
        </select>

        <button
          class="flex items-center space-x-1.5 px-3 py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-sm font-medium transition-colors"
          title="刷新列表"
          @click="fetchRecords()"
        >
          <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': loading }" />
          <span>刷新</span>
        </button>
      </div>
    </div>

    <!-- 分类快捷标签 -->
    <div class="flex items-center gap-2 overflow-x-auto pb-1 text-xs">
      <button
        v-for="cat in categories"
        :key="cat.id"
        class="px-3 py-1.5 rounded-md font-medium transition-all flex items-center space-x-1"
        :class="
          currentCategory === cat.id
            ? 'bg-emerald-600 text-white shadow-sm'
            : 'bg-slate-900 text-slate-400 hover:bg-slate-800 hover:text-slate-200'
        "
        @click="selectCategory(cat.id)"
      >
        <span>{{ cat.icon }}</span>
        <span>{{ cat.label }}</span>
      </button>
      <div class="ml-auto text-xs text-slate-400">
        本页 <span class="font-semibold text-emerald-400">{{ records.length }}</span> 条附件记录
      </div>
    </div>

    <!-- 数据列表区域 -->
    <div class="flex-1 bg-slate-900/60 border border-slate-800/80 rounded-xl overflow-hidden flex flex-col">
      <div class="overflow-x-auto flex-1">
        <table class="w-full text-left text-xs text-slate-300">
          <thead class="bg-slate-900 text-slate-400 uppercase tracking-wider border-b border-slate-800 font-semibold sticky top-0 z-10">
            <tr>
              <th scope="col" class="py-3 px-4">文件名</th>
              <th scope="col" class="py-3 px-3 w-36">来源账号</th>
              <th scope="col" class="py-3 px-3 w-28">月份 / 时间</th>
              <th scope="col" class="py-3 px-3 w-24">文件大小</th>
              <th scope="col" class="py-3 px-3 w-28">BLAKE3 哈希</th>
              <th scope="col" class="py-3 px-4 w-28 text-right">操作</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-slate-800/60">
            <tr
              v-for="item in records"
              :key="item.recordId"
              class="hover:bg-slate-800/40 transition-colors group"
            >
              <!-- 文件名与类型图标 -->
              <td class="py-3 px-4 font-medium text-slate-100 flex items-center space-x-2">
                <span class="text-base select-none">{{ getCategoryEmoji(item.category) }}</span>
                <span class="truncate max-w-xs md:max-w-md" :title="item.originalName">{{ item.originalName }}</span>
              </td>

              <!-- 账号徽标 -->
              <td class="py-3 px-3">
                <span
                  v-if="item.sourceAccountId"
                  class="inline-block px-2 py-0.5 rounded text-[11px] bg-emerald-950/60 text-emerald-400 border border-emerald-800/60 font-mono truncate max-w-[130px]"
                  :title="`${item.sourceAccountName || item.sourceAccountId}（${item.sourceType}）\n${item.sourceAccountId}`"
                >
                  {{ item.sourceAccountName || item.sourceAccountId }}
                  <span v-if="item.sourceConversationName" class="text-sky-400"> · {{ item.sourceConversationName }}</span>
                </span>
                <span v-else class="text-slate-500">通用文件</span>
              </td>

              <!-- 时间 / 月份 -->
              <td class="py-3 px-3 text-slate-400 font-mono">
                {{ item.fileTime || "未知" }}
              </td>

              <!-- 大小 -->
              <td class="py-3 px-3 text-slate-400 font-mono">
                {{ item.formattedSize }}
              </td>

              <!-- 哈希 -->
              <td class="py-3 px-3 font-mono text-slate-400">
                <button
                  class="hover:text-emerald-400 transition-colors"
                  :title="'完整哈希: ' + item.hash + ' (点击复制)'"
                  @click="copyText(item.hash)"
                >
                  {{ item.hash.substring(0, 8) }}...
                </button>
              </td>

              <!-- 操作 -->
              <td class="py-3 px-4 text-right">
                <button
                  class="px-2.5 py-1 rounded bg-slate-800 hover:bg-emerald-600 hover:text-white text-slate-300 font-medium transition-colors text-[11px] inline-flex items-center space-x-1"
                  title="在 Windows 文件资源管理器中高亮选中"
                  @click="revealFile(item.originalPath)"
                >
                  <FolderOpen class="w-3.5 h-3.5" />
                  <span>定位</span>
                </button>
              </td>
            </tr>

            <!-- 空状态提示 -->
            <tr v-if="!loading && records.length === 0">
              <td colspan="6" class="py-16 text-center text-slate-500">
                <FileQuestion class="w-10 h-10 mx-auto mb-2 text-slate-600" />
                <p class="text-sm font-medium">未找到符合条件的文件记录</p>
                <p class="text-xs text-slate-600 mt-1">可在左侧“微信数据源与扫描”中进行微信 4.x 附件入库</p>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
    <div class="flex items-center justify-end gap-3 text-xs text-slate-300">
      <span v-if="error" class="text-red-400 mr-auto">{{ error }}</span>
      <button :disabled="loading || page === 0" class="disabled:opacity-30" @click="changePage(-1)">上一页</button>
      <span>第 {{ page + 1 }} 页</span>
      <button :disabled="loading || !hasNext" class="disabled:opacity-30" @click="changePage(1)">下一页</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { Search, RefreshCw, FolderOpen, FileQuestion } from "lucide-vue-next";
import { searchRecords, revealFileInExplorer, listSourceAccounts } from "../api/tauri";
import type { FileRecordViewDto, SourceAccountDto } from "../types";

// 搜索输入与过滤状态
const keyword = ref("");
const selectedAccount = ref("");
const currentCategory = ref("all");
const loading = ref(false);
const records = ref<FileRecordViewDto[]>([]);
const accountList = ref<SourceAccountDto[]>([]);
const page = ref(0);
const pageSize = 100;
const hasNext = ref(false);
const error = ref("");
let requestId = 0;

let debounceTimer: any = null;

// 分类快捷标签定义
const categories = [
  { id: "all", label: "全部格式", icon: "📁" },
  { id: "doc", label: "文档", icon: "📄" },
  { id: "image", label: "图片", icon: "🖼️" },
  { id: "video", label: "视频", icon: "🎬" },
  { id: "audio", label: "音频", icon: "🎵" },
  { id: "archive", label: "压缩包", icon: "📦" },
  { id: "other", label: "其他", icon: "📎" },
];

/**
 * 根据类别获取对应的展示 Emoji
 */
function getCategoryEmoji(category: string): string {
  switch (category) {
    case "doc":
      return "📄";
    case "image":
      return "🖼️";
    case "video":
      return "🎬";
    case "audio":
      return "🎵";
    case "archive":
      return "📦";
    default:
      return "📎";
  }
}

/**
 * 获取文件列表
 */
async function fetchRecords(reset = true) {
  if (reset) page.value = 0;
  const currentRequest = ++requestId;
  loading.value = true;
  error.value = "";
  try {
    const list = await searchRecords({
      keyword: keyword.value.trim() || undefined,
      category: currentCategory.value !== "all" ? currentCategory.value : undefined,
      ...(selectedAccount.value
        ? {
            sourceType: selectedAccount.value.split("\t")[0],
            sourceAccountId: selectedAccount.value.split("\t")[1],
          }
        : {}),
      limit: pageSize + 1,
      offset: page.value * pageSize,
    });
    if (currentRequest !== requestId) return;
    hasNext.value = list.length > pageSize;
    records.value = list.slice(0, pageSize);
  } catch (err) {
    if (currentRequest === requestId) { error.value = "查询失败：" + err; records.value = []; hasNext.value = false; }
  } finally {
    if (currentRequest === requestId) loading.value = false;
  }
}

/**
 * 搜索框防抖处理
 */
function onSearchInput() {
  if (debounceTimer) {
    clearTimeout(debounceTimer);
  }
  debounceTimer = setTimeout(() => {
    fetchRecords();
  }, 150);
}

/**
 * 清除搜索关键词
 */
function clearKeyword() {
  keyword.value = "";
  fetchRecords();
}

/**
 * 切换分类
 */
function selectCategory(catId: string) {
  currentCategory.value = catId;
  fetchRecords();
}

/**
 * 在 Windows 资源管理器中定位文件
 */
async function revealFile(path: string) {
  try {
    await revealFileInExplorer(path);
  } catch (err) {
    alert("无法定位文件：" + err);
  }
}

/**
 * 复制哈希至剪贴板
 */
function copyText(text: string) {
  navigator.clipboard.writeText(text);
  alert("已复制 BLAKE3 哈希:\n" + text);
}

/** 切换页码并保持当前过滤条件。 */
function changePage(delta: number) {
  page.value += delta;
  fetchRecords(false);
}

onMounted(async () => {
  fetchRecords();
  try { accountList.value = await listSourceAccounts(); }
  catch (err) { error.value = "读取账号失败：" + err; }
});
</script>
