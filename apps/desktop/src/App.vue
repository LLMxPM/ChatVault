<!--
  ChatVault 桌面端根组件
  职责：实现左侧主导航栏、状态指示条及多视图切换（文件检索、微信扫描、存储看板、云端同步）。
-->
<template>
  <div class="flex h-screen w-screen bg-slate-950 text-slate-100 overflow-hidden font-sans">
    <!-- 左侧 Sidebar -->
    <aside class="w-60 bg-slate-900/90 border-r border-slate-800/80 flex flex-col justify-between select-none">
      <!-- 顶部 Logo 与品牌 -->
      <div>
        <div class="p-5 flex items-center space-x-3 border-b border-slate-800/60">
          <img :src="appIcon" alt="拾文" class="w-10 h-10 shrink-0" />
          <div>
            <h1 class="text-sm font-bold tracking-tight text-white flex flex-wrap items-center gap-x-1.5">
              <span>拾文 <span class="text-xs text-slate-400">ChatVault</span></span>
              <span v-if="runtime.version" class="text-[10px] px-1.5 py-0.2 rounded bg-emerald-900/60 text-emerald-400 font-mono">v{{ runtime.version }}</span>
            </h1>
            <p class="text-[10px] text-slate-400">聊天附件归档与检索</p>
          </div>
        </div>

        <!-- 导航菜单项 -->
        <nav class="p-3 space-y-1">
          <button
            v-for="item in navItems"
            :key="item.id"
            class="w-full flex items-center space-x-3 px-3.5 py-2.5 rounded-lg text-xs font-medium transition-all"
            :class="
              currentTab === item.id
                ? 'bg-emerald-600 text-white shadow-md shadow-emerald-950 font-semibold'
                : 'text-slate-400 hover:text-slate-100 hover:bg-slate-800/60'
            "
            @click="currentTab = item.id"
          >
            <component :is="item.icon" class="w-4 h-4" />
            <span>{{ item.label }}</span>
          </button>
        </nav>
        <div v-if="runtime.firstRun && !guideDismissed" class="mx-3 p-3 rounded-lg bg-emerald-950/50 border border-emerald-900 text-xs space-y-2">
          <p class="font-medium text-emerald-300">开始建立你的资料库</p>
          <button class="block text-slate-300 hover:text-white" @click="currentTab = 'scanner'">1. 识别微信来源并扫描文件</button>
          <button class="block text-slate-300 hover:text-white" @click="currentTab = 'sync'">2. 按需连接 WebDAV 归档</button>
          <button class="block text-slate-300 hover:text-white" @click="currentTab = 'settings'">3. 设置定时采集目录</button>
          <p class="text-[11px] text-slate-400">未连接云端也可以本地检索。</p>
          <button class="text-[11px] text-slate-500 hover:text-slate-300" @click="guideDismissed = true">收起引导</button>
        </div>
      </div>

      <!-- 底部状态指示条 -->
      <div class="p-4 border-t border-slate-800/60 text-[11px] text-slate-500 space-y-1">
        <div class="flex items-center space-x-1.5 text-emerald-400">
          <span class="w-2 h-2 rounded-full" :class="runtime.ready ? 'bg-emerald-500' : 'bg-amber-500'"></span>
          <span class="font-medium text-slate-300">{{ runtime.ready ? "本地引擎就绪" : "未连接桌面服务" }}</span>
        </div>
        <p class="text-[10px] text-slate-500 font-mono">SQLite FTS5 + BLAKE3</p>
      </div>
    </aside>

    <!-- 右侧主内容区域 -->
    <main class="flex-1 h-full bg-slate-950 overflow-hidden">
      <LibraryView v-if="currentTab === 'library'" />
      <ScannerView v-else-if="currentTab === 'scanner'" />
      <TasksView v-else-if="currentTab === 'tasks'" />
      <StatsView v-else-if="currentTab === 'stats'" />
      <SyncView v-else-if="currentTab === 'sync'" />
      <SourceManagementView v-else-if="currentTab === 'sources'" />
      <SettingsView v-else-if="currentTab === 'settings'" />
    </main>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { FolderSearch, Search, BarChart3, Cloud, Settings, ListTodo, ContactRound } from "lucide-vue-next";
import appIcon from "./assets/app-icon.png";
import { loadRuntime, runtime } from "./api/runtime";
import LibraryView from "./views/LibraryView.vue";
import ScannerView from "./views/ScannerView.vue";
import StatsView from "./views/StatsView.vue";
import SyncView from "./views/SyncView.vue";
import SettingsView from "./views/SettingsView.vue";
import TasksView from "./views/TasksView.vue";
import SourceManagementView from "./views/SourceManagementView.vue";

const currentTab = ref<"library" | "scanner" | "sources" | "tasks" | "stats" | "sync" | "settings">("library");
const guideDismissed = ref(false);

/** 初始化运行信息；空资料库首先展示来源扫描入口。 */
onMounted(async () => {
  await loadRuntime();
  if (runtime.firstRun) currentTab.value = "scanner";
});

const navItems = [
  { id: "library", label: "文件库与检索", icon: FolderSearch },
  { id: "scanner", label: "微信来源与扫描", icon: Search },
  { id: "sources", label: "来源管理", icon: ContactRound },
  { id: "tasks", label: "任务中心", icon: ListTodo },
  { id: "stats", label: "存储看板与去重", icon: BarChart3 },
  { id: "sync", label: "WebDAV 归档同步", icon: Cloud },
  { id: "settings", label: "设置", icon: Settings },
] as const;
</script>
