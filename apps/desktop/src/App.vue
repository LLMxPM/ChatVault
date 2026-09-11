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
          <div class="w-8 h-8 rounded-lg bg-gradient-to-tr from-emerald-600 to-teal-400 flex items-center justify-center text-white shadow-md shadow-emerald-950">
            <Archive class="w-5 h-5" />
          </div>
          <div>
            <h1 class="text-sm font-bold tracking-tight text-white flex items-center space-x-1.5">
              <span>ChatVault</span>
              <span class="text-[10px] px-1.5 py-0.2 rounded bg-emerald-900/60 text-emerald-400 font-mono">v0.5</span>
            </h1>
            <p class="text-[10px] text-slate-400">微信 4.x 本地归档与检索</p>
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
      </div>

      <!-- 底部状态指示条 -->
      <div class="p-4 border-t border-slate-800/60 text-[11px] text-slate-500 space-y-1">
        <div class="flex items-center space-x-1.5 text-emerald-400">
          <span class="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></span>
          <span class="font-medium text-slate-300">本地引擎就绪</span>
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
      <SettingsView v-else-if="currentTab === 'settings'" />
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { Archive, FolderSearch, Search, BarChart3, Cloud, Settings, ListTodo } from "lucide-vue-next";
import LibraryView from "./views/LibraryView.vue";
import ScannerView from "./views/ScannerView.vue";
import StatsView from "./views/StatsView.vue";
import SyncView from "./views/SyncView.vue";
import SettingsView from "./views/SettingsView.vue";
import TasksView from "./views/TasksView.vue";

const currentTab = ref<"library" | "scanner" | "tasks" | "stats" | "sync" | "settings">("library");

const navItems = [
  { id: "library", label: "文件库与检索", icon: FolderSearch },
  { id: "scanner", label: "微信来源与扫描", icon: Search },
  { id: "tasks", label: "任务中心", icon: ListTodo },
  { id: "stats", label: "存储看板与去重", icon: BarChart3 },
  { id: "sync", label: "WebDAV 归档同步", icon: Cloud },
  { id: "settings", label: "设置", icon: Settings },
] as const;
</script>
