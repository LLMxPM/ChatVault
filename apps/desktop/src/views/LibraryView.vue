<!--
  ChatVault 文件库与全文检索视图
  职责：中文即输即搜、多维筛选、按内容折叠分页、位置状态、打开/定位/下载与来源标注入口。
-->
<template>
  <div class="flex h-full flex-col gap-4 p-6">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div>
        <h2 class="text-cv-page text-cv-text">文件库</h2>
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
        <UiSelect v-model="selectedAccount" @change="fetchObjects()">
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
      <UiButton variant="secondary" size="md" :loading="loading" @click="fetchObjects()">
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
        第 {{ page + 1 }} 页 · 本页 {{ objects.length }} 个对象
      </span>
    </div>

    <div class="min-h-0 flex-1 overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface">
      <div class="h-full overflow-auto">
        <table class="w-full text-left text-cv-body">
          <thead class="sticky top-0 z-10 border-b border-cv-border bg-cv-surface-2 text-cv-caption text-cv-text-2">
            <tr>
              <th class="px-4 py-2.5 font-medium">文件名</th>
              <th class="w-28 px-3 py-2.5 font-medium">位置</th>
              <th class="w-28 px-3 py-2.5 font-medium">来源数</th>
              <th class="w-32 px-3 py-2.5 font-medium">时间</th>
              <th class="w-24 px-3 py-2.5 font-medium">大小</th>
              <th class="w-56 px-4 py-2.5 text-right font-medium">操作</th>
            </tr>
          </thead>
          <tbody>
            <template v-for="item in objects" :key="item.objectId">
              <tr
                class="cursor-pointer border-b border-cv-border/60 transition-colors last:border-0 hover:bg-cv-surface-2"
                :class="expandedId === item.objectId ? 'bg-cv-surface-2' : ''"
                @click="toggleExpand(item)"
              >
                <td class="max-w-[280px] px-4 py-2.5">
                  <div class="flex items-center gap-2">
                    <ChevronDown
                      v-if="item.sourceCount > 1"
                      class="h-3.5 w-3.5 shrink-0 text-cv-text-3 transition-transform"
                      :class="expandedId === item.objectId ? 'rotate-0' : '-rotate-90'"
                    />
                    <span v-else class="w-3.5 shrink-0" />
                    <component :is="categoryIcon(item.category)" class="h-4 w-4 shrink-0 text-cv-text-3" />
                    <span class="truncate font-medium text-cv-text" :title="item.originalName">
                      {{ item.originalName }}
                    </span>
                  </div>
                </td>
                <td class="px-3 py-2.5">
                  <span
                    class="inline-block rounded-cv px-1.5 py-0.5 text-cv-caption font-medium"
                    :class="locationClass(item.location)"
                    :title="locationTitle(item.location)"
                  >
                    {{ locationLabel(item.location) }}
                  </span>
                </td>
                <td class="px-3 py-2.5 text-cv-caption text-cv-text-2">
                  {{ item.sourceCount > 1 ? `${item.sourceCount} 条` : "1 条" }}
                </td>
                <td class="px-3 py-2.5 font-mono text-cv-caption text-cv-text-2">
                  {{ formatDateTime(item.fileTime, { fallback: "未知" }) }}
                </td>
                <td class="px-3 py-2.5 font-mono text-cv-caption text-cv-text-2">{{ item.formattedSize }}</td>
                <td class="px-4 py-2.5 text-right" @click.stop>
                  <div class="flex items-center justify-end gap-1">
                    <UiButton
                      v-if="item.openPath"
                      size="sm"
                      variant="ghost"
                      :loading="busyKey === item.objectId + ':open'"
                      @click="handleOpen(item)"
                    >
                      <template #icon><ExternalLink class="h-3.5 w-3.5" /></template>
                      打开
                    </UiButton>
                    <UiButton
                      v-if="item.openPath"
                      size="sm"
                      variant="ghost"
                      @click="revealFile(item.openPath!)"
                    >
                      <template #icon><FolderOpen class="h-3.5 w-3.5" /></template>
                      定位
                    </UiButton>
                    <UiButton
                      v-if="!item.openPath && canDownload(item)"
                      size="sm"
                      variant="ghost"
                      :loading="busyKey === item.objectId + ':dl'"
                      @click="handleDownload(item)"
                    >
                      <template #icon><Download class="h-3.5 w-3.5" /></template>
                      下载
                    </UiButton>
                    <UiButton size="sm" variant="ghost" :title="item.hash" @click="copyText(item.hash)">
                      <template #icon><Copy class="h-3.5 w-3.5" /></template>
                    </UiButton>
                  </div>
                </td>
              </tr>
              <tr v-if="expandedId === item.objectId">
                <td colspan="6" class="border-b border-cv-border/60 bg-cv-surface-2/80 px-4 py-2">
                  <div v-if="sourcesLoading" class="py-2 text-cv-caption text-cv-text-3">加载来源…</div>
                  <div v-else-if="expandedSources.length === 0" class="py-2 text-cv-caption text-cv-text-3">
                    暂无来源明细
                  </div>
                  <div v-else class="space-y-1.5 py-1">
                    <div
                      v-for="src in expandedSources"
                      :key="src.recordId"
                      class="flex flex-wrap items-center gap-2 text-cv-caption"
                    >
                      <span
                        class="inline-block max-w-[140px] truncate rounded-cv bg-cv-surface px-1.5 py-0.5 text-cv-text-2"
                      >
                        {{ src.sourceAccountName || src.sourceAccountId || "通用文件" }}
                        <span v-if="src.sourceConversationName" class="text-cv-accent">
                          · {{ src.sourceConversationName }}
                        </span>
                      </span>
                      <span class="max-w-[220px] truncate text-cv-text-2" :title="src.originalName">
                        {{ src.originalName }}
                      </span>
                      <span class="font-mono text-cv-text-3" :title="src.originalPath">
                        {{ src.hasLocalPath ? src.originalPath || "本地可用" : "本机无路径" }}
                      </span>
                      <span class="font-mono text-cv-text-3" :title="src.deviceId">
                        设备 {{ src.deviceId }}
                      </span>
                      <span class="font-mono text-cv-text-3">
                        {{ formatDateTime(src.fileTime, { fallback: "—" }) }}
                      </span>
                      <UiButton
                        v-if="src.hasLocalPath && src.originalPath"
                        size="sm"
                        variant="ghost"
                        @click="revealFile(src.originalPath)"
                      >
                        定位
                      </UiButton>
                    </div>
                  </div>
                </td>
              </tr>
            </template>
            <tr v-if="!loading && objects.length === 0">
              <td colspan="6" class="py-16 text-center">
                <FileQuestion class="mx-auto mb-2 h-10 w-10 text-cv-text-3" />
                <p class="text-cv-body font-medium text-cv-text">未找到符合条件的文件</p>
                <p class="mt-1 text-cv-caption text-cv-text-2">可先到「任务」配置范围并立即运行</p>
                <UiButton class="mt-3" variant="primary" size="sm" @click="navigateTo('tasks')">
                  去任务
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

    <SourceLabelPanel v-if="showSources" @close="showSources = false" @changed="onSourcesChanged" />
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
  ExternalLink,
  Download,
  Copy,
  ChevronDown,
} from "lucide-vue-next";
import UiInput from "../components/ui/UiInput.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import UiButton from "../components/ui/UiButton.vue";
import SourceLabelPanel from "../components/SourceLabelPanel.vue";
import {
  searchObjects,
  listObjectSources,
  revealFileInExplorer,
  openFileWithSystem,
  downloadObject,
  listSourceAccounts,
  getVaultStats,
} from "../api/tauri";
import { pushToast } from "../composables/useToast";
import { navigateTo } from "../composables/useNav";
import { formatDateTime } from "../utils/format";
import type {
  FileObjectViewDto,
  FileSourceDto,
  FileLocation,
  SourceAccountDto,
  VaultStatsDto,
} from "../types";

const keyword = ref("");
const selectedAccount = ref("");
const currentCategory = ref("all");
const loading = ref(false);
const objects = ref<FileObjectViewDto[]>([]);
const accountList = ref<SourceAccountDto[]>([]);
const stats = ref<VaultStatsDto | null>(null);
const page = ref(0);
const pageSize = 100;
const hasNext = ref(false);
const error = ref("");
const showSources = ref(false);
const expandedId = ref<string | null>(null);
const expandedSources = ref<FileSourceDto[]>([]);
const sourcesLoading = ref(false);
const busyKey = ref("");
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
  // 仅有远端时提供下载；本地可打开时不展示下载
  return item.location === "remote" || (item.location === "both" && !item.openPath);
}

/** 查询对象列表；reset 为 true 时回到第一页。 */
async function fetchObjects(reset = true) {
  if (reset) page.value = 0;
  const currentRequest = ++requestId;
  loading.value = true;
  error.value = "";
  try {
    const list = await searchObjects({
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
    objects.value = list.slice(0, pageSize);
    if (expandedId.value && !objects.value.some((o) => o.objectId === expandedId.value)) {
      expandedId.value = null;
      expandedSources.value = [];
    }
  } catch (err) {
    if (currentRequest === requestId) {
      pushToast({ tone: "danger", title: "查询失败", description: String(err) });
      objects.value = [];
      hasNext.value = false;
    }
  } finally {
    if (currentRequest === requestId) loading.value = false;
  }
}

/** 搜索防抖。 */
function onSearchInput() {
  if (debounceTimer) window.clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => fetchObjects(), 150);
}

/** 清空关键词并重新查询。 */
function clearKeyword() {
  keyword.value = "";
  fetchObjects();
}

/** 切换分类筛选。 */
function selectCategory(catId: string) {
  currentCategory.value = catId;
  fetchObjects();
}

/** 展开/收起来源明细。 */
async function toggleExpand(item: FileObjectViewDto) {
  if (expandedId.value === item.objectId) {
    expandedId.value = null;
    expandedSources.value = [];
    return;
  }
  expandedId.value = item.objectId;
  expandedSources.value = [];
  sourcesLoading.value = true;
  try {
    expandedSources.value = await listObjectSources(item.objectId);
  } catch (err) {
    pushToast({ tone: "danger", title: "加载来源失败", description: String(err) });
  } finally {
    sourcesLoading.value = false;
  }
}

/** 打开本地文件。 */
async function handleOpen(item: FileObjectViewDto) {
  if (!item.openPath) return;
  busyKey.value = item.objectId + ":open";
  try {
    await openFileWithSystem(item.openPath);
  } catch (err) {
    pushToast({ tone: "danger", title: "无法打开文件", description: String(err) });
  } finally {
    busyKey.value = "";
  }
}

/** 在资源管理器中定位文件。 */
async function revealFile(path: string) {
  try {
    await revealFileInExplorer(path);
  } catch (err) {
    pushToast({ tone: "danger", title: "无法定位文件", description: String(err) });
  }
}

/** 从远端下载对象。 */
async function handleDownload(item: FileObjectViewDto) {
  busyKey.value = item.objectId + ":dl";
  try {
    const result = await downloadObject(item.objectId, item.originalName);
    pushToast({
      tone: "success",
      title: "已下载",
      description: result.savedPath,
    });
  } catch (err) {
    pushToast({ tone: "danger", title: "下载失败", description: String(err) });
  } finally {
    busyKey.value = "";
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
  fetchObjects(false);
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

/** 来源标注保存后刷新筛选与列表。 */
async function onSourcesChanged() {
  await loadMeta();
  await fetchObjects(false);
}

onMounted(() => {
  fetchObjects();
  loadMeta();
});

onBeforeUnmount(() => {
  if (debounceTimer) window.clearTimeout(debounceTimer);
});
</script>
