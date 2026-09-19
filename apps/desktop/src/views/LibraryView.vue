<!--
  ChatVault 文件库与全文检索视图
  职责：中文即输即搜、多维筛选、按内容折叠分页、位置状态、打开/定位/下载、下载目录入口、
  详情侧栏、多选批量导出/释放缓存/删除本机文件与来源标注入口。
-->
<template>
  <div class="flex h-full flex-col gap-3 p-6">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div class="flex items-center gap-3">
        <h2 class="text-cv-page text-cv-text">文件库</h2>
        <div class="flex shrink-0 items-center rounded-cv border border-cv-border bg-cv-surface p-0.5">
          <button
            class="rounded-cv px-2.5 py-1 text-cv-caption transition-colors"
            :class="visibilityMode === 'normal' ? 'bg-cv-accent-soft text-cv-accent' : 'text-cv-text-2 hover:text-cv-text'"
            @click="setVisibilityMode('normal')"
          >
            正常
          </button>
          <button
            class="rounded-cv px-2.5 py-1 text-cv-caption transition-colors"
            :class="visibilityMode === 'hidden' ? 'bg-cv-accent-soft text-cv-accent' : 'text-cv-text-2 hover:text-cv-text'"
            @click="setVisibilityMode('hidden')"
          >
            隐藏
          </button>
        </div>
      </div>
      <div class="flex items-center gap-2 text-cv-caption text-cv-text-2">
        <template v-if="stats">
          <span>记录 {{ stats.totalRecords }}</span>
          <span class="text-cv-text-3">·</span>
          <span>对象 {{ stats.uniqueObjects }}</span>
          <span class="text-cv-text-3">·</span>
          <span>去重节省 {{ stats.formattedSavedBytes }}</span>
        </template>
        <UiButton size="sm" variant="secondary" @click="showSources = true">来源标注</UiButton>
        <UiButton
          size="sm"
          variant="secondary"
          title="打开下载目录"
          aria-label="打开下载目录"
          @click="handleOpenDownloadDir"
        >
          下载目录
        </UiButton>
        <UiButton
          v-if="pendingRemotePurges > 0"
          size="sm"
          variant="secondary"
          :loading="batchBusy === 'purge-remote'"
          @click="handleCleanupPurgeRemote"
        >
          清理网盘残留 {{ pendingRemotePurges }}
        </UiButton>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <div class="relative min-w-[200px] flex-1">
        <Search class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-cv-text-3" />
        <UiInput
          v-model="keyword"
          class="pl-9"
          placeholder="搜索文件名、关键词…"
          @input="onSearchInput"
          @compositionstart="onCompositionStart"
          @compositionend="onCompositionEnd"
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
      <div class="w-36 shrink-0">
        <UiSelect v-model="currentCategory" @change="onFilterChange">
          <option v-for="cat in categories" :key="cat.id" :value="cat.id">
            {{ cat.id === "all" ? "全部类型" : cat.label }}
          </option>
        </UiSelect>
      </div>
      <div class="w-40 shrink-0">
        <UiSelect v-model="selectedAccount" @change="onFilterChange">
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
      <div class="w-32 shrink-0">
        <UiSelect v-model="selectedLocation" @change="onFilterChange">
          <option value="">全部位置</option>
          <option value="local">本地</option>
          <option value="remote">远程</option>
          <option value="both">本地+远程</option>
          <option value="missing">失效</option>
        </UiSelect>
      </div>
      <div class="w-36 shrink-0">
        <UiSelect v-model="selectedSort" @change="onFilterChange">
          <option value="file_time_desc">时间 ↓</option>
          <option value="file_time_asc">时间 ↑</option>
          <option value="size_desc">大小 ↓</option>
          <option value="size_asc">大小 ↑</option>
          <option value="name_asc">名称 A→Z</option>
          <option value="name_desc">名称 Z→A</option>
        </UiSelect>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-1.5">
      <template v-if="activeChips.length > 0">
        <span
          v-for="chip in activeChips"
          :key="chip.key"
          class="inline-flex items-center gap-1 rounded-cv bg-cv-surface-2 px-2 py-0.5 text-cv-caption text-cv-text-2"
        >
          {{ chip.label }}
          <button class="text-cv-text-3 hover:text-cv-text" @click="chip.clear">
            <X class="h-3 w-3" />
          </button>
        </span>
        <button class="text-cv-caption text-cv-accent hover:underline" @click="clearAllFilters">
          清空筛选
        </button>
      </template>
      <span v-else class="text-cv-caption text-cv-text-3">暂无筛选项</span>
      <span class="ml-auto flex items-center gap-1.5 text-cv-caption text-cv-text-3">
        共 {{ total }} 个对象 · 本页 {{ objects.length }}
        <UiButton
          size="sm"
          variant="ghost"
          :loading="loading"
          title="刷新"
          aria-label="刷新"
          @click="fetchObjects()"
        >
          <template #icon><RefreshCw class="h-3.5 w-3.5" /></template>
        </UiButton>
      </span>
    </div>

    <div
      v-if="selectedIds.size > 0"
      class="flex flex-wrap items-center gap-2 rounded-cv border border-cv-accent/30 bg-cv-accent-soft/40 px-3 py-2"
    >
      <span class="text-cv-caption font-medium text-cv-accent">已选 {{ selectedIds.size }} 项</span>
      <template v-if="visibilityMode === 'normal'">
        <UiButton size="sm" variant="primary" :loading="batchBusy === 'dl'" @click="handleBatchDownload">
          下载
        </UiButton>
        <UiButton
          size="sm"
          variant="secondary"
          :loading="batchBusy === 'cache'"
          @click="handleBatchReleaseCache"
        >
          释放缓存
        </UiButton>
        <UiButton size="sm" variant="secondary" :loading="batchBusy === 'hide'" @click="handleBatchHide">
          隐藏
        </UiButton>
        <UiButton
          size="sm"
          variant="danger"
          :loading="batchBusy === 'local'"
          @click="handleBatchDeleteLocal"
        >
          删除本机文件
        </UiButton>
      </template>
      <template v-else>
        <UiButton size="sm" variant="primary" :loading="batchBusy === 'dl'" @click="handleBatchDownload">
          下载
        </UiButton>
        <UiButton
          size="sm"
          variant="secondary"
          :loading="batchBusy === 'restore'"
          @click="handleBatchRestore"
        >
          恢复
        </UiButton>
        <UiButton size="sm" variant="danger" :loading="batchBusy === 'purge'" @click="handleBatchPurge">
          彻底删除
        </UiButton>
      </template>
      <button class="ml-auto text-cv-caption text-cv-text-2 hover:text-cv-text" @click="clearSelection">
        取消选择
      </button>
    </div>

    <div class="flex min-h-0 flex-1 overflow-hidden">
      <div class="min-w-0 flex-1 overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface">
        <div class="h-full overflow-auto">
          <table class="w-full table-fixed text-left text-cv-body">
            <thead class="sticky top-0 z-10 border-b border-cv-border bg-cv-surface-2 text-cv-caption text-cv-text-2">
              <tr>
                <th class="w-10 px-2 py-2.5 font-medium">
                  <input
                    type="checkbox"
                    class="accent-cv-accent"
                    :checked="allPageSelected"
                    :indeterminate="somePageSelected && !allPageSelected"
                    @change="toggleSelectPage"
                  />
                </th>
                <th class="px-2 py-2.5 font-medium">文件名</th>
                <th class="w-28 px-2 py-2.5 font-medium">位置</th>
                <th class="w-24 px-2 py-2.5 font-medium">来源数</th>
                <th class="w-32 px-2 py-2.5 font-medium">时间</th>
                <th class="w-20 px-2 py-2.5 font-medium">大小</th>
                <th class="w-36 px-2 py-2.5 text-right font-medium">操作</th>
              </tr>
            </thead>
            <tbody>
              <template v-for="item in objects" :key="item.objectId">
                <tr
                  class="cursor-pointer border-b border-cv-border/60 transition-colors last:border-0 hover:bg-cv-surface-2"
                  :class="selectedItem?.objectId === item.objectId ? 'bg-cv-surface-2' : ''"
                  @click="openDetail(item)"
                >
                  <td class="px-2 py-2.5" @click.stop>
                    <input
                      type="checkbox"
                      class="accent-cv-accent"
                      :checked="selectedIds.has(item.objectId)"
                      @change="toggleSelect(item.objectId)"
                    />
                  </td>
                  <td class="max-w-[240px] px-2 py-2.5">
                    <div class="flex items-center gap-2">
                      <component :is="categoryIcon(item.category)" class="h-4 w-4 shrink-0 text-cv-text-3" />
                      <span class="truncate font-medium text-cv-text" :title="item.originalName">
                        {{ item.originalName }}
                      </span>
                    </div>
                  </td>
                  <td class="px-2 py-2.5">
                    <span
                      class="inline-block rounded-cv px-1.5 py-0.5 text-cv-caption font-medium"
                      :class="locationClass(item.location)"
                      :title="locationTitle(item.location)"
                    >
                      {{ locationLabel(item.location) }}
                    </span>
                  </td>
                  <td class="px-2 py-2.5 text-cv-caption text-cv-text-2">
                    {{ item.sourceCount > 1 ? item.sourceCount + " 条" : "1 条" }}
                  </td>
                  <td class="px-2 py-2.5 font-mono text-cv-caption text-cv-text-2">
                    {{ formatDateTime(item.fileTime, { fallback: "未知" }) }}
                  </td>
                  <td class="px-2 py-2.5 font-mono text-cv-caption text-cv-text-2">
                    {{ item.formattedSize }}
                  </td>
                  <td class="px-2 py-2.5 text-right" @click.stop>
                    <div class="flex items-center justify-end gap-1">
                      <UiButton v-if="item.openPath" size="sm" variant="ghost" @click="handleOpen(item)">
                        打开
                      </UiButton>
                      <UiButton
                        v-else-if="canDownload(item)"
                        size="sm"
                        variant="ghost"
                        @click="handleDownload(item)"
                      >
                        下载
                      </UiButton>
                      <UiButton size="sm" variant="ghost" :title="item.hash" @click="copyText(item.hash)">
                        <Copy class="h-3.5 w-3.5" />
                      </UiButton>
                    </div>
                  </td>
                </tr>
              </template>
              <tr v-if="!loading && objects.length === 0">
                <td colspan="7" class="py-16 text-center">
                  <FileQuestion class="mx-auto mb-2 h-10 w-10 text-cv-text-3" />
                  <p class="text-cv-body font-medium text-cv-text">{{ emptyTitle }}</p>
                  <p class="mt-1 text-cv-caption text-cv-text-2">{{ emptyHint }}</p>
                  <UiButton
                    v-if="hasActiveFilters"
                    class="mt-3"
                    variant="secondary"
                    size="sm"
                    @click="clearAllFilters"
                  >
                    清空筛选
                  </UiButton>
                  <UiButton v-else class="mt-3" variant="primary" size="sm" @click="navigateTo('tasks')">
                    去任务
                  </UiButton>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>

    <div class="flex items-center justify-between gap-3">
      <span v-if="error" class="text-cv-caption text-cv-danger">{{ error }}</span>
      <div v-else />
      <div class="flex shrink-0 flex-nowrap items-center gap-2">
        <label class="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-cv-caption text-cv-text-2">
          每页
          <span class="w-20 shrink-0">
            <UiSelect v-model="pageSizeKey" @change="onPageSizeChange">
              <option v-for="n in pageSizeOptions" :key="n" :value="String(n)">{{ n }}</option>
            </UiSelect>
          </span>
        </label>
        <UiButton size="sm" variant="secondary" :disabled="loading || page === 0" @click="changePage(-1)">
          上一页
        </UiButton>
        <span class="whitespace-nowrap text-cv-caption text-cv-text-2">第 {{ page + 1 }} 页</span>
        <UiButton size="sm" variant="secondary" :disabled="loading || !hasNext" @click="changePage(1)">
          下一页
        </UiButton>
      </div>
    </div>

    <SourceLabelPanel v-if="showSources" @close="showSources = false" @changed="onSourcesChanged" />
    <LibraryDetailPanel
      v-if="selectedItem"
      :item="selectedItem"
      :hidden="visibilityMode === 'hidden'"
      @close="selectedItem = null"
      @changed="onDetailChanged"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onActivated, onBeforeUnmount, watch, type Component } from "vue";
import {
  Search,
  RefreshCw,
  FileQuestion,
  X,
  FileText,
  Image,
  Film,
  Music,
  Archive,
  Paperclip,
  Folder,
  Copy,
} from "lucide-vue-next";
import UiInput from "../components/ui/UiInput.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import UiButton from "../components/ui/UiButton.vue";
import SourceLabelPanel from "../components/SourceLabelPanel.vue";
import LibraryDetailPanel from "../components/LibraryDetailPanel.vue";
import {
  searchObjects,
  openFileWithSystem,
  revealFileInExplorer,
  downloadObject,
  downloadObjects,
  openDownloadDir,
  releaseObjectCache,
  deleteObjectLocalFiles,
  libraryHideObjects,
  libraryRestoreObjects,
  libraryPurgeObjects,
  listSourceAccounts,
  getVaultStats,
  countPendingRemotePurges,
  cleanupPurgeRemote,
} from "../api/tauri";
import { pushToast } from "../composables/useToast";
import { confirmAction } from "../composables/useConfirm";
import { navigateTo, libraryFocusQuery } from "../composables/useNav";
import { formatDateTime } from "../utils/format";
import type {
  FileObjectViewDto,
  FileLocation,
  SourceAccountDto,
  VaultStatsDto,
  BatchResultDto,
} from "../types";

const keyword = ref("");
const selectedAccount = ref("");
const selectedLocation = ref("");
const selectedSort = ref("file_time_desc");
const currentCategory = ref("all");
/** 列表可见性视图：正常 / 隐藏 */
const visibilityMode = ref<"normal" | "hidden">("normal");
const loading = ref(false);
const objects = ref<FileObjectViewDto[]>([]);
const accountList = ref<SourceAccountDto[]>([]);
const stats = ref<VaultStatsDto | null>(null);
const page = ref(0);
const pageSizeOptions = [50, 100, 200, 500] as const;
const pageSizeKey = ref("100");
const pageSize = computed(() => Number(pageSizeKey.value) || 100);
const total = ref(0);
const hasNext = computed(() => (page.value + 1) * pageSize.value < total.value);
const error = ref("");
const showSources = ref(false);
const selectedItem = ref<FileObjectViewDto | null>(null);
const selectedIds = ref(new Set<string>());
const batchBusy = ref("");
/** 仍待重试删除的网盘残留对象数 */
const pendingRemotePurges = ref(0);
let requestId = 0;
let debounceTimer: number | undefined;
/** 中文输入法组合态：组合文字尚未提交时不触发搜索。 */
let isComposing = false;
const SEARCH_DEBOUNCE_MS = 350;

const categories = [
  { id: "all", label: "全部类型" },
  { id: "doc", label: "文档" },
  { id: "image", label: "图片" },
  { id: "video", label: "视频" },
  { id: "audio", label: "音频" },
  { id: "archive", label: "压缩包" },
  { id: "other", label: "其他" },
];

const hasActiveFilters = computed(() => {
  return (
    !!keyword.value.trim() ||
    !!selectedAccount.value ||
    !!selectedLocation.value ||
    currentCategory.value !== "all"
  );
});

const emptyTitle = computed(() => {
  if (visibilityMode.value === "hidden") {
    return hasActiveFilters.value ? "未找到符合条件的隐藏文件" : "暂无隐藏文件";
  }
  return hasActiveFilters.value ? "未找到符合条件的文件" : "文件库暂无内容";
});
const emptyHint = computed(() => {
  if (visibilityMode.value === "hidden") {
    return hasActiveFilters.value ? "可清空筛选后重试" : "可在「正常」列表中隐藏文件";
  }
  return hasActiveFilters.value ? "可清空筛选后重试" : "可先到「任务」配置范围并立即运行";
});

const allPageSelected = computed(
  () => objects.value.length > 0 && objects.value.every((o) => selectedIds.value.has(o.objectId)),
);
const somePageSelected = computed(() =>
  objects.value.some((o) => selectedIds.value.has(o.objectId)),
);

const activeChips = computed(() => {
  const chips: { key: string; label: string; clear: () => void }[] = [];
  if (keyword.value.trim()) {
    chips.push({
      key: "kw",
      label: `关键词: ${keyword.value.trim()}`,
      clear: () => {
        keyword.value = "";
        fetchObjects();
      },
    });
  }
  if (currentCategory.value !== "all") {
    const label =
      categories.find((c) => c.id === currentCategory.value)?.label || currentCategory.value;
    chips.push({
      key: "cat",
      label: `类型: ${label}`,
      clear: () => {
        currentCategory.value = "all";
        onFilterChange();
      },
    });
  }
  if (selectedAccount.value) {
    chips.push({
      key: "acc",
      label: "已选来源",
      clear: () => {
        selectedAccount.value = "";
        onFilterChange();
      },
    });
  }
  if (selectedLocation.value) {
    const map: Record<string, string> = {
      local: "本地",
      remote: "远程",
      both: "本地+远程",
      missing: "失效",
    };
    chips.push({
      key: "loc",
      label: `位置: ${map[selectedLocation.value] || selectedLocation.value}`,
      clear: () => {
        selectedLocation.value = "";
        onFilterChange();
      },
    });
  }
  return chips;
});

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

function locationLabel(loc: FileLocation): string {
  switch (loc) {
    case "local":
      return "本地";
    case "remote":
      return "远程";
    case "both":
      return "本地+远程";
    case "missing":
      return "失效";
    default:
      return loc;
  }
}

function locationClass(loc: FileLocation): string {
  switch (loc) {
    case "both":
      return "bg-cv-accent-soft text-cv-accent";
    case "local":
      return "bg-cv-surface-2 text-cv-text-2";
    case "remote":
      return "bg-cv-surface-2 text-cv-warning";
    case "missing":
      return "bg-cv-surface-2 text-cv-danger";
    default:
      return "bg-cv-surface-2 text-cv-text-2";
  }
}

function locationTitle(loc: FileLocation): string {
  switch (loc) {
    case "local":
      return "本机存在，尚未确认远端归档";
    case "remote":
      return "仅在远端，可下载到本地";
    case "both":
      return "本机可读且已远端归档";
    case "missing":
      return "本地路径失效且无远端归档";
    default:
      return "";
  }
}

function canDownload(item: FileObjectViewDto): boolean {
  return item.location === "remote" || (item.location === "both" && !item.openPath);
}

function clearSelection() {
  selectedIds.value = new Set();
}

function toggleSelect(id: string) {
  const next = new Set(selectedIds.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  selectedIds.value = next;
}

function toggleSelectPage() {
  const next = new Set(selectedIds.value);
  if (allPageSelected.value) {
    objects.value.forEach((o) => next.delete(o.objectId));
  } else {
    objects.value.forEach((o) => next.add(o.objectId));
  }
  selectedIds.value = next;
}

function openDetail(item: FileObjectViewDto) {
  selectedItem.value = item;
}

function onDetailChanged() {
  void fetchObjects(false);
  void loadMeta();
}

async function fetchObjects(reset = true) {
  if (reset) page.value = 0;
  const currentRequest = ++requestId;
  loading.value = true;
  error.value = "";
  clearSelection();
  try {
    const account = selectedAccount.value
      ? {
          sourceType: selectedAccount.value.split("\t")[0],
          sourceAccountId: selectedAccount.value.split("\t")[1],
        }
      : {};
    const result = await searchObjects({
      keyword: keyword.value.trim() || undefined,
      category: currentCategory.value !== "all" ? currentCategory.value : undefined,
      ...account,
      location: (selectedLocation.value || undefined) as FileLocation | undefined,
      hidden: visibilityMode.value === "hidden",
      sort: selectedSort.value,
      limit: pageSize.value,
      offset: page.value * pageSize.value,
    });
    if (currentRequest !== requestId) return;
    objects.value = result.items;
    total.value = result.total;
    if (
      selectedItem.value &&
      !objects.value.some((o) => o.objectId === selectedItem.value?.objectId)
    ) {
      selectedItem.value = null;
    }
  } catch (err) {
    if (currentRequest === requestId) {
      pushToast({ tone: "danger", title: "查询失败", description: String(err) });
      objects.value = [];
      total.value = 0;
    }
  } finally {
    if (currentRequest === requestId) loading.value = false;
  }
}

function onSearchInput() {
  if (isComposing) return;
  if (debounceTimer) window.clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => fetchObjects(), SEARCH_DEBOUNCE_MS);
}

/** 标记输入法进入组合输入，避免半成品关键词触发请求。 */
function onCompositionStart() {
  isComposing = true;
  if (debounceTimer) window.clearTimeout(debounceTimer);
}

/** 输入法提交关键词后立即执行一次搜索。 */
function onCompositionEnd() {
  isComposing = false;
  onSearchInput();
}

function clearKeyword() {
  keyword.value = "";
  fetchObjects();
}

function onFilterChange() {
  fetchObjects();
}

function setVisibilityMode(mode: "normal" | "hidden") {
  if (visibilityMode.value === mode) return;
  visibilityMode.value = mode;
  selectedItem.value = null;
  fetchObjects();
}

function onPageSizeChange() {
  page.value = 0;
  fetchObjects(false);
}

function clearAllFilters() {
  keyword.value = "";
  selectedAccount.value = "";
  selectedLocation.value = "";
  selectedSort.value = "file_time_desc";
  currentCategory.value = "all";
  fetchObjects();
}

function changePage(delta: number) {
  page.value += delta;
  fetchObjects(false);
}

async function handleOpen(item: FileObjectViewDto) {
  if (!item.openPath) return;
  try {
    await openFileWithSystem(item.openPath, item.extension);
  } catch (err) {
    pushToast({ tone: "danger", title: "无法打开文件", description: String(err) });
  }
}

async function handleDownload(item: FileObjectViewDto) {
  try {
    const result = await downloadObject(item.objectId, item.originalName);
    pushToast({
      tone: result.skipped ? "info" : "success",
      title: result.skipped ? "已存在，跳过重复下载" : "已下载",
      description: result.savedPath,
      actions: [
        {
          label: "打开",
          onClick: () => {
            void openFileWithSystem(result.savedPath).catch((err) => {
              pushToast({ tone: "danger", title: "无法打开文件", description: String(err) });
            });
          },
        },
        {
          label: "定位",
          onClick: () => {
            void revealFileInExplorer(result.savedPath).catch((err) => {
              pushToast({ tone: "danger", title: "无法定位文件", description: String(err) });
            });
          },
        },
        {
          label: "下载目录",
          onClick: () => {
            void openDownloadDir().catch((err) => {
              pushToast({ tone: "danger", title: "无法打开下载目录", description: String(err) });
            });
          },
        },
      ],
    });
  } catch (err) {
    pushToast({ tone: "danger", title: "下载失败", description: String(err) });
  }
}

/** 打开配置的用户下载目录。 */
async function handleOpenDownloadDir() {
  try {
    await openDownloadDir();
  } catch (err) {
    pushToast({ tone: "danger", title: "无法打开下载目录", description: String(err) });
  }
}

function reportBatch(
  title: string,
  result: BatchResultDto,
  options?: {
    actions?: { label: string; onClick: () => void }[];
    /** 下载类批量：区分新下载/跳过 */
    downloadStyle?: boolean;
  },
) {
  const failed = result.items.filter((i) => i.status === "failed");
  let description: string;
  if (options?.downloadStyle) {
    const skipped = result.items.filter((i) => i.status === "skipped").length;
    const downloaded = result.items.filter((i) => i.status === "ok").length;
    const parts: string[] = [`新下载 ${downloaded} 个`];
    if (skipped > 0) parts.push(`跳过 ${skipped} 个`);
    if (result.failedCount > 0) parts.push(`失败 ${result.failedCount} 个`);
    if (failed[0]?.error) parts.push(failed[0].error);
    description = parts.join("，");
  } else {
    description =
      result.failedCount === 0
        ? `成功 ${result.okCount} 个` +
          (result.releasedBytes > 0 ? `，释放 ${result.releasedBytes} 字节` : "")
        : `成功 ${result.okCount} 个，失败 ${result.failedCount} 个` +
          (failed[0]?.error ? `：${failed[0].error}` : "");
  }
  pushToast({
    tone: result.failedCount === 0 ? "success" : result.okCount > 0 ? "warning" : "danger",
    title,
    description,
    actions: options?.actions,
  });
}

async function handleBatchDownload() {
  const ids = [...selectedIds.value];
  if (ids.length === 0) return;
  batchBusy.value = "dl";
  try {
    const result = await downloadObjects(ids);
    const hasOk = result.items.some((i) => i.status === "ok" || i.status === "skipped");
    const downloadDirActions = hasOk
      ? [
          {
            label: "打开下载目录",
            onClick: () => {
              void openDownloadDir().catch((err) => {
                pushToast({
                  tone: "danger",
                  title: "无法打开下载目录",
                  description: String(err),
                });
              });
            },
          },
        ]
      : undefined;
    reportBatch("批量下载完成", result, {
      actions: downloadDirActions,
      downloadStyle: true,
    });
  } catch (err) {
    pushToast({ tone: "danger", title: "批量下载失败", description: String(err) });
  } finally {
    batchBusy.value = "";
  }
}

async function handleBatchHide() {
  const ids = [...selectedIds.value];
  if (ids.length === 0) return;
  const ok = await confirmAction({
    title: "隐藏所选文件",
    description: `将隐藏已选 ${ids.length} 个文件。\n默认列表不再显示，可在「隐藏」视图恢复。\n不会中断备份，也不会删除本机原文件。`,
    confirmLabel: "隐藏",
  });
  if (!ok) return;
  batchBusy.value = "hide";
  try {
    const result = await libraryHideObjects(ids);
    reportBatch("隐藏完成", result);
    await fetchObjects(false);
  } catch (err) {
    pushToast({ tone: "danger", title: "隐藏失败", description: String(err) });
  } finally {
    batchBusy.value = "";
  }
}

async function handleBatchRestore() {
  const ids = [...selectedIds.value];
  if (ids.length === 0) return;
  batchBusy.value = "restore";
  try {
    const result = await libraryRestoreObjects(ids);
    reportBatch("恢复完成", result);
    await fetchObjects(false);
  } catch (err) {
    pushToast({ tone: "danger", title: "恢复失败", description: String(err) });
  } finally {
    batchBusy.value = "";
  }
}

async function handleBatchPurge() {
  const ids = [...selectedIds.value];
  if (ids.length === 0) return;
  const ok = await confirmAction({
    title: "彻底删除所选文件",
    description:
      `将彻底删除已选 ${ids.length} 个已隐藏文件。\n` +
      "会删除本机库记录，并尝试删除网盘归档内容。\n" +
      "不会删除微信/电脑上的原文件；源文件仍在时下次扫描可能重新入库。\n" +
      "此操作不可恢复。",
    confirmLabel: "彻底删除",
  });
  if (!ok) return;
  batchBusy.value = "purge";
  try {
    const result = await libraryPurgeObjects(ids);
    reportBatch("彻底删除完成", result);
    if (result.failedCount > 0 || result.items.some((i) => i.status === "partial")) {
      pushToast({
        tone: "warning",
        title: "存在网盘残留",
        description: "部分远端对象清理失败，可稍后点「清理网盘残留」重试。",
      });
    }
    await fetchObjects(false);
    await loadMeta();
  } catch (err) {
    pushToast({ tone: "danger", title: "彻底删除失败", description: String(err) });
  } finally {
    batchBusy.value = "";
  }
}

/** 重试删除彻底删除后仍残留在网盘的对象。 */
async function handleCleanupPurgeRemote() {
  const ok = await confirmAction({
    title: "清理网盘残留",
    description:
      `将重试删除 ${pendingRemotePurges.value} 个已彻底删除但仍残留在网盘的内容对象。\n` +
      "仅删除本应用 Vault 下的内容对象，不会影响其他文件。",
    confirmLabel: "开始清理",
    danger: true,
  });
  if (!ok) return;
  batchBusy.value = "purge-remote";
  try {
    const result = await cleanupPurgeRemote();
    reportBatch("网盘残留清理完成", result);
    await loadMeta();
  } catch (err) {
    pushToast({ tone: "danger", title: "清理网盘残留失败", description: String(err) });
  } finally {
    batchBusy.value = "";
  }
}

async function handleBatchReleaseCache() {
  const ids = [...selectedIds.value];
  if (ids.length === 0) return;
  batchBusy.value = "cache";
  try {
    const result = await releaseObjectCache(ids);
    reportBatch("释放缓存完成", result);
    void fetchObjects(false);
  } catch (err) {
    pushToast({ tone: "danger", title: "释放缓存失败", description: String(err) });
  } finally {
    batchBusy.value = "";
  }
}

async function handleBatchDeleteLocal() {
  const ids = [...selectedIds.value];
  if (ids.length === 0) return;
  const ok = await confirmAction({
    title: "删除本机原文件",
    description:
      `将永久删除已选 ${ids.length} 个文件的本机原文件与缓存副本。\n` +
      "仅当文件已完成网盘归档与元数据同步时才允许删除。\n" +
      "不会删除 WebDAV 归档内容；若归档尚未完成，后端会拒绝执行。",
    confirmLabel: "删除",
    danger: true,
  });
  if (!ok) return;
  batchBusy.value = "local";
  try {
    const result = await deleteObjectLocalFiles(ids);
    reportBatch("删除本机文件完成", result);
    void fetchObjects(false);
  } catch (err) {
    pushToast({ tone: "danger", title: "删除失败", description: String(err) });
  } finally {
    batchBusy.value = "";
  }
}

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    pushToast({ tone: "success", title: "已复制哈希", description: text.substring(0, 16) + "…" });
  } catch (err) {
    pushToast({ tone: "danger", title: "复制失败", description: String(err) });
  }
}

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
  try {
    pendingRemotePurges.value = await countPendingRemotePurges();
  } catch {
    pendingRemotePurges.value = 0;
  }
}

async function onSourcesChanged() {
  await loadMeta();
  await fetchObjects(false);
}

onMounted(() => {
  fetchObjects();
  loadMeta();
  if (libraryFocusQuery.value) {
    keyword.value = libraryFocusQuery.value;
    libraryFocusQuery.value = null;
    void fetchObjects();
  }
});

onActivated(() => {
  void fetchObjects(false);
  void loadMeta();
});

watch(libraryFocusQuery, (q) => {
  if (!q) return;
  keyword.value = q;
  libraryFocusQuery.value = null;
  void fetchObjects();
});

onBeforeUnmount(() => {
  if (debounceTimer) window.clearTimeout(debounceTimer);
});
</script>
