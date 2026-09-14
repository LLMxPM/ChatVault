<!--
  ChatVault 文件库详情抽屉
  职责：以右侧抽屉展示对象摘要、时间、来源与操作（打开/定位/下载/复制/释放/删原文件）。
  样式与 SourceLabelPanel 对齐：全屏遮罩 + 右侧滑出。
-->
<template>
  <div class="fixed inset-0 z-40 flex justify-end bg-black/30" @click.self="emit('close')">
    <div class="flex h-full w-full max-w-2xl flex-col border-l border-cv-border bg-cv-bg shadow-xl">
      <header class="flex items-start justify-between gap-2 border-b border-cv-border px-4 py-3">
        <div class="min-w-0">
          <div class="flex items-center gap-2">
            <component :is="categoryIcon(item.category)" class="h-4 w-4 shrink-0 text-cv-text-3" />
            <h3 class="truncate text-cv-section text-cv-text" :title="item.originalName">
              {{ item.originalName }}
            </h3>
          </div>
          <div class="mt-1 flex flex-wrap items-center gap-2 text-cv-caption text-cv-text-2">
            <span
              class="inline-block rounded-cv px-1.5 py-0.5 font-medium"
              :class="locationClass(item.location)"
            >
              {{ locationLabel(item.location) }}
            </span>
            <span>{{ item.formattedSize }}</span>
            <span v-if="item.extension">.{{ item.extension }}</span>
            <span>{{ item.sourceCount }} 条来源</span>
          </div>
        </div>
        <UiButton size="sm" variant="ghost" class="shrink-0" @click="emit('close')">关闭</UiButton>
      </header>

      <div class="min-h-0 flex-1 space-y-4 overflow-auto p-4">
        <section class="rounded-cv-lg border border-cv-border bg-cv-surface px-4 py-3">
          <div class="flex items-start gap-2 font-mono text-cv-caption text-cv-text-2">
            <span class="w-16 shrink-0 text-cv-text-3">Hash</span>
            <span class="min-w-0 flex-1 break-all">{{ item.hash }}</span>
            <button class="shrink-0 text-cv-accent hover:underline" @click="copyText(item.hash)">
              复制
            </button>
          </div>
        </section>

        <section class="rounded-cv-lg border border-cv-border bg-cv-surface px-4 py-3">
          <h4 class="text-cv-caption font-medium text-cv-text-3">时间</h4>
          <div class="mt-1.5 space-y-1 text-cv-caption text-cv-text-2">
            <div>
              文件时间
              <span class="font-mono text-cv-text">{{ formatDateTime(item.fileTime) }}</span>
              <span v-if="item.timeSource" class="ml-1 text-cv-text-3">({{ item.timeSource }})</span>
            </div>
            <div>
              发现时间
              <span class="font-mono text-cv-text">{{ formatDateTime(item.discoveredAt) }}</span>
            </div>
          </div>
        </section>

        <section
          class="flex min-h-0 flex-col overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface"
        >
          <header
            class="flex items-center justify-between border-b border-cv-border px-3 py-2.5"
          >
            <div>
              <h4 class="text-cv-body font-medium text-cv-text">来源</h4>
              <p class="text-cv-caption text-cv-text-3">
                {{ loading ? "加载中…" : sources.length + " 条记录" }}
              </p>
            </div>
            <UiButton size="sm" variant="ghost" :loading="loading" @click="loadSources">刷新</UiButton>
          </header>
          <div class="min-h-0 max-h-[50vh] overflow-y-auto">
            <p
              v-if="!loading && sources.length === 0"
              class="p-6 text-center text-cv-caption text-cv-text-3"
            >
              暂无来源明细
            </p>
            <div
              v-for="src in sources"
              :key="src.recordId"
              class="border-b border-cv-border/60 px-3 py-2.5 last:border-0"
            >
              <div class="flex min-w-0 flex-wrap items-center gap-2">
                <span
                  class="max-w-[180px] shrink-0 truncate rounded-cv bg-cv-surface-2 px-1.5 py-0.5 text-cv-caption text-cv-text-2"
                  :title="src.sourceAccountName || src.sourceAccountId || '通用文件'"
                >
                  {{ src.sourceAccountName || src.sourceAccountId || "通用文件" }}
                  <span v-if="src.sourceConversationName" class="text-cv-accent">
                    · {{ src.sourceConversationName }}
                  </span>
                </span>
                <span
                  class="min-w-0 flex-1 truncate text-cv-body font-medium text-cv-text"
                  :title="src.originalName"
                >
                  {{ src.originalName }}
                </span>
                <span class="shrink-0 text-cv-caption text-cv-text-3">
                  {{ formatDateTime(src.fileTime, { fallback: "—" }) }}
                </span>
                <UiButton
                  v-if="src.hasLocalPath && src.originalPath"
                  size="sm"
                  variant="ghost"
                  class="shrink-0"
                  @click="revealFile(src.originalPath)"
                >
                  定位
                </UiButton>
              </div>
              <div class="mt-1.5 flex min-w-0 items-center gap-2 text-cv-caption text-cv-text-3">
                <span class="min-w-0 flex-1 truncate font-mono" :title="src.originalPath">
                  {{ src.hasLocalPath ? src.originalPath || "本地可用" : "本机无路径" }}
                </span>
                <span class="max-w-[140px] shrink-0 truncate">
                  {{ src.deviceName || shortDeviceId(src.deviceId) }}
                </span>
                <span
                  v-if="src.isLocal"
                  class="shrink-0 rounded-cv bg-cv-accent-soft px-1.5 py-0.5 font-medium text-cv-accent"
                >
                  本机
                </span>
              </div>
            </div>
          </div>
        </section>
      </div>

      <footer
        class="flex flex-wrap items-center gap-1.5 border-t border-cv-border bg-cv-surface px-4 py-3"
      >
        <UiButton
          v-if="item.openPath"
          size="sm"
          variant="secondary"
          :loading="busy === 'open'"
          @click="onOpen"
        >
          <template #icon><ExternalLink class="h-3.5 w-3.5" /></template>
          打开
        </UiButton>
        <UiButton
          v-if="item.openPath"
          size="sm"
          variant="secondary"
          @click="revealFile(item.openPath!)"
        >
          <template #icon><FolderOpen class="h-3.5 w-3.5" /></template>
          定位
        </UiButton>
        <UiButton size="sm" variant="secondary" :loading="busy === 'dl'" @click="onDownload">
          <template #icon><Download class="h-3.5 w-3.5" /></template>
          下载
        </UiButton>
        <UiButton size="sm" variant="secondary" :loading="busy === 'cache'" @click="onReleaseCache">
          释放缓存
        </UiButton>
        <UiButton size="sm" variant="danger" :loading="busy === 'local'" @click="onDeleteLocal">
          删除本机文件
        </UiButton>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, type Component } from "vue";
import {
  FileText,
  Image,
  Film,
  Music,
  Archive,
  Paperclip,
  Folder,
  ExternalLink,
  FolderOpen,
  Download,
} from "lucide-vue-next";
import UiButton from "./ui/UiButton.vue";
import {
  listObjectSources,
  openFileWithSystem,
  downloadObject,
  releaseObjectCache,
  deleteObjectLocalFiles,
  revealFileInExplorer,
} from "../api/tauri";
import { pushToast } from "../composables/useToast";
import { confirmAction } from "../composables/useConfirm";
import { formatDateTime } from "../utils/format";
import type { FileObjectViewDto, FileSourceDto, FileLocation } from "../types";

const props = defineProps<{ item: FileObjectViewDto }>();
const emit = defineEmits<{
  close: [];
  changed: [];
}>();

const sources = ref<FileSourceDto[]>([]);
const loading = ref(false);
const busy = ref("");

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
    case "remote":
      return "bg-cv-surface-2 text-cv-warning";
    case "missing":
      return "bg-cv-surface-2 text-cv-danger";
    default:
      return "bg-cv-surface-2 text-cv-text-2";
  }
}

function shortDeviceId(id: string): string {
  if (!id) return "—";
  return id.length > 12 ? `${id.slice(0, 8)}…` : id;
}

async function loadSources() {
  loading.value = true;
  sources.value = [];
  try {
    sources.value = await listObjectSources(props.item.objectId);
  } catch (err) {
    pushToast({ tone: "danger", title: "加载来源失败", description: String(err) });
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.item.objectId,
  () => {
    void loadSources();
  },
  { immediate: true },
);

async function onOpen() {
  if (!props.item.openPath) return;
  busy.value = "open";
  try {
    await openFileWithSystem(props.item.openPath, props.item.extension);
  } catch (err) {
    pushToast({ tone: "danger", title: "无法打开文件", description: String(err) });
  } finally {
    busy.value = "";
  }
}

async function revealFile(path: string) {
  try {
    await revealFileInExplorer(path);
  } catch (err) {
    pushToast({ tone: "danger", title: "无法定位文件", description: String(err) });
  }
}

async function onDownload() {
  busy.value = "dl";
  try {
    const result = await downloadObject(props.item.objectId, props.item.originalName);
    pushToast({ tone: "success", title: "已下载", description: result.savedPath });
  } catch (err) {
    pushToast({ tone: "danger", title: "下载失败", description: String(err) });
  } finally {
    busy.value = "";
  }
}

async function onReleaseCache() {
  busy.value = "cache";
  try {
    const result = await releaseObjectCache([props.item.objectId]);
    const item = result.items[0];
    if (result.okCount > 0 && item?.status === "ok") {
      pushToast({
        tone: "success",
        title: "已释放缓存",
        description: `释放 ${item.releasedBytes ?? 0} 字节`,
      });
      emit("changed");
    } else {
      pushToast({
        tone: "danger",
        title: "释放失败",
        description: item?.error || "尚未完成远端归档，无法释放缓存",
      });
    }
  } catch (err) {
    pushToast({ tone: "danger", title: "释放失败", description: String(err) });
  } finally {
    busy.value = "";
  }
}

async function onDeleteLocal() {
  const ok = await confirmAction({
    title: "删除本机原文件",
    description: `将删除「${props.item.originalName}」的本机原文件与缓存副本。\n不会删除 WebDAV 归档内容。`,
    confirmLabel: "删除",
    danger: true,
  });
  if (!ok) return;
  busy.value = "local";
  try {
    const result = await deleteObjectLocalFiles([props.item.objectId]);
    const item = result.items[0];
    if (result.okCount > 0 && item?.status === "ok") {
      pushToast({
        tone: "success",
        title: "已删除本机文件",
        description: `释放约 ${item.releasedBytes ?? 0} 字节`,
      });
      emit("changed");
    } else {
      pushToast({
        tone: "danger",
        title: "删除失败",
        description: item?.error || "未知错误",
      });
    }
  } catch (err) {
    pushToast({ tone: "danger", title: "删除失败", description: String(err) });
  } finally {
    busy.value = "";
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
</script>
