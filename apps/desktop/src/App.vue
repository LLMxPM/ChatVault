<!--
  ChatVault 桌面端根组件
  职责：自定义标题栏、三项主导航、主题初始化、全局 Toast/Confirm 与引擎就绪状态。
  侧边栏在采集源/WebDAV 未配置时展示轻量提示，替代原先的首次引导。
-->
<template>
  <div class="flex h-screen w-screen flex-col overflow-hidden bg-cv-bg font-sans text-cv-text">
    <AppTitleBar />

    <div class="flex min-h-0 flex-1">
      <aside class="flex w-56 shrink-0 flex-col border-r border-cv-border bg-cv-surface select-none">
        <div>
          <nav class="space-y-0.5 p-2">
            <button
              v-for="item in navItems"
              :key="item.id"
              class="flex w-full items-center gap-2.5 rounded-cv px-3 py-2 text-cv-body transition-colors"
              :class="
                currentTab === item.id
                  ? 'bg-cv-accent-soft font-medium text-cv-accent'
                  : 'text-cv-text-2 hover:bg-cv-surface-2 hover:text-cv-text'
              "
              @click="navigateTo(item.id)"
            >
              <component :is="item.icon" class="h-4 w-4 shrink-0" />
              <span class="min-w-0 flex-1 truncate text-left">{{ item.label }}</span>
              <span
                v-if="navHintCount(item.id)"
                class="h-1.5 w-1.5 shrink-0 rounded-full bg-cv-warning"
                aria-hidden="true"
              />
            </button>
          </nav>

          <div
            v-if="setupHints.length"
            class="mx-2 mt-1 space-y-1.5 rounded-cv-lg border border-cv-border bg-cv-surface-2 p-3"
          >
            <p class="text-cv-caption font-medium text-cv-text">待完成配置</p>
            <button
              v-for="hint in setupHints"
              :key="hint.id"
              class="flex w-full items-center gap-1.5 text-cv-caption text-cv-text-2 transition-colors hover:text-cv-accent"
              @click="navigateTo(hint.tab)"
            >
              <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-cv-warning" aria-hidden="true" />
              <span>{{ hint.label }}</span>
            </button>
          </div>
        </div>

        <div class="mt-auto border-t border-cv-border p-4">
          <div class="flex items-center gap-1.5">
            <span
              class="h-1.5 w-1.5 rounded-full"
              :class="runtime.ready ? 'bg-cv-success' : 'bg-cv-warning'"
            />
            <span class="text-cv-caption text-cv-text-2">
              {{ runtime.ready ? "本地引擎就绪" : "未连接桌面服务" }}
            </span>
          </div>
        </div>
      </aside>

      <main class="h-full min-w-0 flex-1 overflow-hidden">
        <keep-alive>
          <LibraryView v-if="currentTab === 'library'" key="library" />
          <TasksView v-else-if="currentTab === 'tasks'" key="tasks" />
          <SettingsView v-else-if="currentTab === 'settings'" key="settings" />
        </keep-alive>
      </main>
    </div>

    <UiToast />
    <UiConfirm />
  </div>
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount, watch } from "vue";
import { FolderSearch, ListTodo, Settings } from "lucide-vue-next";
import AppTitleBar from "./components/AppTitleBar.vue";
import { loadRuntime, runtime } from "./api/runtime";
import { initTheme, disposeTheme } from "./composables/useTheme";
import { currentTab, navigateTo, type AppTab } from "./composables/useNav";
import { refreshSetupStatus, setupHints } from "./composables/useSetupStatus";
import LibraryView from "./views/LibraryView.vue";
import TasksView from "./views/TasksView.vue";
import SettingsView from "./views/SettingsView.vue";
import UiToast from "./components/ui/UiToast.vue";
import UiConfirm from "./components/ui/UiConfirm.vue";

const navItems: { id: AppTab; label: string; icon: typeof FolderSearch }[] = [
  { id: "library", label: "文件库", icon: FolderSearch },
  { id: "tasks", label: "任务", icon: ListTodo },
  { id: "settings", label: "设置", icon: Settings },
];

/** 导航项上的待配置角标数量。 */
function navHintCount(tab: AppTab): number {
  return setupHints.value.filter((hint) => hint.tab === tab).length;
}

onMounted(async () => {
  initTheme();
  await loadRuntime();
  void refreshSetupStatus();
});

// 配置页保存后切回时同步侧边栏提示
watch(currentTab, () => {
  void refreshSetupStatus();
});

onBeforeUnmount(() => {
  disposeTheme();
});
</script>
