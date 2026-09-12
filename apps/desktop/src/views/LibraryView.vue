<!--
  ChatVault 文件库与全文检索视图
  职责：中文即输即搜、多维筛选、分页列表与资源管理器定位。
-->
<template>
  <div class="flex h-full flex-col gap-4 p-6">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div>
        <h2 class="text-cv-page text-cv-text">文件库</h2>
        <p class="mt-0.5 text-cv-caption text-cv-text-2">按文件名、类型、来源检索已入库附件</p>
      </div>
      <div class="flex items-center gap-2 text-cv-caption text-cv-text-2">
        <template v-if="stats">
          <span>记录 {{ stats.totalRecords }}</span>
          <span class="text-cv-text-3">·</span>
          <span>对象 {{ stats.uniqueObjects }}</span>
          <span class="text-cv-text-3">·</span>
          <span>去重节省 {{ stats.formattedSavedBytes }}</span>
        </template>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <div class="relative min-w-[220px] flex-1">
        <Search class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-cv-text-3" />
        <UiInput
          v-model="keyword"
          class="pl-9"
          placeholder="搜索文件名、关键词…"
          @input="onSearchInput"
        />
        <button
          v-if="keyword"
          class="absolute right-2.5 top-1/2 -translate-y-1/2 text-cv-text-3 hover:text-cv-text"
          aria-label="清除"
          @click="clearKeyword"
        >
          <X class="h-3.5 w-3.5" />
        </button>
      </div>
      <div class="w-48 shrink-0">
        <UiSelect v-model="selectedAccount" @change="fetchRecords()">
          <option value="">全部来源</option>
          <option
            v-for="acc in accountList"
            :key="acc.sourceType + ':' + acc.sourceAccountId"
            :value="acc.sourceType + '\t' + acc.sourceAccountId"
          >
            {{ acc.effectiveName }}
          </option>
        </UiSelect>
      </div>
      <UiButton variant="secondary" size="md" :loading="loading" @click="fetchRecords()">
        <template #icon><RefreshCw class="h-4 w-4" /></template>
        刷新
      </UiButton>
    </div>

    <div class="flex flex-wrap items-center gap-1.5">
      <button
        v-for="cat in categories"
        :key="cat.id"
        class="rounded-cv px-2.5 py-1 text-cv-caption transition-colors"
        :class="
          currentCategory === cat.id
            ? 'bg-cv-accent-soft font-medium text-cv-accent'
            : 'text-cv-text-2 hover:bg-cv-surface-2 hover:text-cv-text'
        "
        @click="selectCategory(cat.id)"
      >
        {{ cat.label }}
      </button>
      <span class="ml-auto text-cv-caption text-cv-text-3">
        第 {{ page + 1 }} 页 · 本页 {{ records.length }} 条
      </span>
    </div>

    <div class="min-h-0 flex-1 overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface">
      <div class="h-full overflow-auto">
        <table class="w-full text-left text-cv-body">
          <thead class="sticky top-0 z-10 border-b border-cv-border bg-cv-surface-2 text-cv-caption text-cv-text-2">
            <tr>
              <th class="px-4 py-2.5 font-medium">文件名</th>
              <th class="w-36 px-3 py-2.5 font-medium">来源</th>
              <th class="w-32 px-3 py-2.5 font-medium">时间</th>
              <th class="w-24 px-3 py-2.5 font-medium">大小</th>
              <th class="w-28 px-3 py-2.5 font-medium">哈希</th>
              <th class="w-24 px-4 py-2.5 text-right font-medium">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="item in records"
              :key="item.recordId"
              class="border-b border-cv-border/60 transition-colors last:border-0 hover:bg-cv-surface-2"
            >
              <td class="max-w-[280px] px-4 py-2.5">
                <div class="flex items-center gap-2">
                  <component :is="categoryIcon(item.category)" class="h-4 w-4 shrink-0 text-cv-text-3" />
                  <span class="truncate font-medium text-cv-text" :title="item.originalName">
                    {{ item.originalName }}
                  </span>
                </div>
              </td>
              <td class="px-3 py-2.5">
                <span
                  v-if="item.sourceAccountId"
                  class="inline-block max-w-[130px] truncate rounded-cv bg-cv-surface-2 px-1.5 py-0.5 text-cv-caption text-cv-text-2"
                  :title="`${item.sourceAccountName || item.sourceAccountId}${item.sourceConversationName ? ' · ' + item.sourceConversationName : ''}`"
                >
                  {{ item.sourceAccountName || item.sourceAccountId }}
                  <span v-if="item.sourceConversationName" class="text-cv-accent"> · {{ item.sourceConversationName }}</span>
                </span>
                <span v-else class="text-cv-text-3">通用文件</span>
              </td>
              <td class="px-3 py-2.5 font-mono text-cv-caption text-cv-text-2">
                {{ item.fileTime || "未知" }}
              </td>
              <td class="px-3 py-2.5 font-mono text-cv-caption text-cv-text-2">{{ item.formattedSize }}</td>
              <td class="px-3 py-2.5 font-mono text-cv-caption">
                <button
                  class="text-cv-text-2 hover:text-cv-accent"
                  :title="item.hash"
                  @click="copyText(item.hash)"
                >
                  {{ item.hash.substring(0, 8) }}…
                </button>
              </td>
              <td class="px-4 py-2.5 text-right">
                <UiButton size="sm" variant="ghost" @click="revealFile(item.originalPath)">
                  <template #icon><FolderOpen class="h-3.5 w-3.5" /></template>
                  定位
                </UiButton>
              </td>
            </tr>
            <tr v-if="!loading && records.length === 0">
              <td colspan="6" class="py-16 text-center">
                <FileQuestion class="mx-auto mb-2 h-10 w-10 text-cv-text-3" />
                <p class="text-cv-body font-medium text-cv-text">未找到符合条件的文件</p>
                <p class="mt-1 text-cv-caption text-cv-text-2">可先到「采集」扫描微信附件入库</p>
                <UiButton class="mt-3" variant="primary" size="sm" @click="navigateTo('collect')">
                  去采集
                </UiButton>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="flex items-center justify-between gap-3">
      <span v-if="error" class="text-cv-caption text-cv-danger">{{ error }}</span>
      <div v-else />
      <div class="flex items-center gap-2">
        <UiButton size="sm" variant="secondary" :disabled="loading || page === 0" @click="changePage(-1)">
          上一页
        </UiButton>
        <span class="text-cv-caption text-cv-text-2">第 {{ page + 1 }} 页</span>
        <UiButton size="sm" variant="secondary" :disabled="loading || !hasNext" @click="changePage(1)">
          下一页
        </UiButton>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, type Component } from "vue";
import {
  Search,
  RefreshCw,
  FolderOpen,
  FileQuestion,
  X,
  FileText,
  Image,
  Film,
  Music,
  Archive,
  Paperclip,
  Folder,
} from "lucide-vue-next";
import UiInput from "../components/ui/UiInput.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import UiButton from "../components/ui/UiButton.vue";
import { searchRecords, revealFileInExplorer, listSourceAccounts, getVaultStats } from "../api/tauri";
import { pushToast } from "../composables/useToast";
import { navigateTo } from "../composables/useNav";
import type { FileRecordViewDto, SourceAccountDto, VaultStatsDto } from "../types";

const keyword = ref("");
const selectedAccount = ref("");
const currentCategory = ref("all");
const loading = ref(false);
const records = ref<FileRecordViewDto[]>([]);
const accountList = ref<SourceAccountDto[]>([]);
const stats = ref<VaultStatsDto | null>(null);
const page = ref(0);
const pageSize = 100;
const hasNext = ref(false);
const error = ref("");
let requestId = 0;
let debounceTimer: number | undefined;

const categories = [
  { id: "all", label: "全部" },
  { id: "doc", label: "文档" },
  { id: "image", label: "图片" },
  { id: "video", label: "视频" },
  { id: "audio", label: "音频" },
  { id: "archive", label: "压缩包" },
  { id: "other", label: "其他" },
];

/** 按类别返回 lucide 图标组件。 */
function categoryIcon(category: string): Component {
  switch (category) {
    case "doc":
      return FileText;
    case "image":
      return Image;
    case "video":
      return Film;
    case "audio":
      return Music;
    case "archive":
      return Archive;
    case "other":
      return Paperclip;
    default:
      return Folder;
  }
}

/** 查询文件列表；reset 为 true 时回到第一页。 */
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
    if (currentRequest === requestId) {
      pushToast({ tone: "danger", title: "查询失败", description: String(err) });
      records.value = [];
      hasNext.value = false;
    }
  } finally {
    if (currentRequest === requestId) loading.value = false;
  }
}

/** 搜索防抖。 */
function onSearchInput() {
  if (debounceTimer) window.clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => fetchRecords(), 150);
}

/** 清空关键词并重新查询。 */
function clearKeyword() {
  keyword.value = "";
  fetchRecords();
}

/** 切换分类筛选。 */
function selectCategory(catId: string) {
  currentCategory.value = catId;
  fetchRecords();
}

/** 在资源管理器中定位文件。 */
async function revealFile(path: string) {
  try {
    await revealFileInExplorer(path);
  } catch (err) {
    pushToast({ tone: "danger", title: "无法定位文件", description: String(err) });
  }
}

/** 复制哈希到剪贴板。 */
async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    pushToast({ tone: "success", title: "已复制哈希", description: text.substring(0, 16) + "…" });
  } catch (err) {
    pushToast({ tone: "danger", title: "复制失败", description: String(err) });
  }
}

/** 翻页。 */
function changePage(delta: number) {
  page.value += delta;
  fetchRecords(false);
}

/** 刷新账号下拉与库摘要。 */
async function loadMeta() {
  try {
    accountList.value = await listSourceAccounts();
  } catch (err) {
    pushToast({ tone: "danger", title: "读取来源账号失败", description: String(err) });
  }
  try {
    stats.value = await getVaultStats();
  } catch {
    stats.value = null;
  }
}

onMounted(() => {
  fetchRecords();
  loadMeta();
});

onBeforeUnmount(() => {
  if (debounceTimer) window.clearTimeout(debounceTimer);
});

</script>

