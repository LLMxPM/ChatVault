<!--
  拾文自定义标题栏
  职责：无边框窗口下的品牌区、拖拽移动、双击最大化与最小化/还原/关闭。
  约束：Windows + Tauri 2 使用 decorations:false；控制权限见 capabilities/default.json。
-->
<template>
  <header
    class="flex h-10 shrink-0 select-none items-stretch border-b border-cv-border bg-cv-surface"
  >
    <div
      class="flex min-w-0 flex-1 items-center gap-2.5 px-3"
      @mousedown="onDragAreaMouseDown"
      @dblclick="toggleMaximize"
    >
      <img :src="appIcon" alt="" class="h-5 w-5 shrink-0" draggable="false" />
      <span class="truncate text-cv-body font-medium text-cv-text">拾文</span>
      <span
        v-if="runtime.version"
        class="rounded-cv bg-cv-surface-2 px-1 font-mono text-[10px] text-cv-text-3"
      >
        v{{ runtime.version }}
      </span>
    </div>

    <div class="flex items-stretch">
      <button
        type="button"
        class="flex w-12 items-center justify-center text-cv-text-2 transition-colors hover:bg-cv-surface-2 hover:text-cv-text"
        aria-label="最小化"
        @click="minimize"
      >
        <Minus class="h-3.5 w-3.5" />
      </button>
      <button
        type="button"
        class="flex w-12 items-center justify-center text-cv-text-2 transition-colors hover:bg-cv-surface-2 hover:text-cv-text"
        :aria-label="isMaximized ? '还原' : '最大化'"
        @click="toggleMaximize"
      >
        <Copy v-if="isMaximized" class="h-3.5 w-3.5" />
        <Square v-else class="h-3 w-3" />
      </button>
      <button
        type="button"
        class="flex w-12 items-center justify-center text-cv-text-2 transition-colors hover:bg-cv-danger hover:text-white"
        aria-label="关闭"
        @click="close"
      >
        <X class="h-3.5 w-3.5" />
      </button>
    </div>
  </header>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { Minus, Square, Copy, X } from "lucide-vue-next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import appIcon from "../assets/app-icon.png";
import { runtime } from "../api/runtime";

const isMaximized = ref(false);
const appWindow = resolveWindow();
let unlistenResize: (() => void) | null = null;

/** 在 Tauri 桌面环境解析当前窗口；浏览器预览返回 null。 */
function resolveWindow() {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

/** 同步窗口最大化状态，供还原图标切换。 */
async function syncMaximized(): Promise<void> {
  if (!appWindow) return;
  try {
    isMaximized.value = await appWindow.isMaximized();
  } catch {
    // 权限或预览环境失败时保持默认图标
  }
}

/** 左键按下品牌区时启动窗口拖拽；避免依赖 data-tauri-drag-region 以免干扰双击最大化。 */
async function onDragAreaMouseDown(event: MouseEvent): Promise<void> {
  if (event.button !== 0 || !appWindow) return;
  try {
    await appWindow.startDragging();
  } catch {
    // 预览环境忽略
  }
}

/** 最小化主窗口。 */
async function minimize(): Promise<void> {
  try {
    await appWindow?.minimize();
  } catch {
    // 预览环境忽略
  }
}

/** 最大化 / 还原切换。 */
async function toggleMaximize(): Promise<void> {
  if (!appWindow) return;
  try {
    if (await appWindow.isMaximized()) {
      await appWindow.unmaximize();
    } else {
      await appWindow.maximize();
    }
    await syncMaximized();
  } catch {
    // 预览环境忽略
  }
}

/** 关闭主窗口并退出应用。 */
async function close(): Promise<void> {
  try {
    await appWindow?.close();
  } catch {
    // 预览环境忽略
  }
}

onMounted(async () => {
  if (!appWindow) return;
  await syncMaximized();
  try {
    unlistenResize = await appWindow.onResized(() => {
      void syncMaximized();
    });
  } catch {
    unlistenResize = null;
  }
});

onBeforeUnmount(() => {
  unlistenResize?.();
  unlistenResize = null;
});
</script>
