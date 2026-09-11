<!--
  ChatVault 任务中心
  职责：展示上传/校验任务状态，支持重试、暂停与筛选。
-->
<template>
  <div class="h-full flex flex-col p-6 space-y-4">
    <div class="flex items-center justify-between border-b border-slate-800 pb-3">
      <div>
        <h2 class="text-lg font-bold text-white">任务中心</h2>
        <p class="text-xs text-slate-400 mt-0.5">查看归档任务状态，失败可重试，进行中可暂停。</p>
      </div>
      <button
        class="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs"
        @click="refresh"
      >
        <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': loading }" />
        <span>刷新</span>
      </button>
    </div>

    <div class="flex items-center space-x-2">
      <select
        v-model="statusFilter"
        class="bg-slate-900 border border-slate-800 rounded-lg px-3 py-1.5 text-sm text-slate-200 focus:outline-none focus:border-emerald-500"
        @change="refresh"
      >
        <option value="">全部状态</option>
        <option value="queued">待上传</option>
        <option value="retryable_failed">可重试失败</option>
        <option value="missing">本地缺失</option>
        <option value="paused">已暂停</option>
        <option value="backed_up">已校验归档</option>
      </select>
      <span class="text-xs text-slate-500">共 {{ tasks.length }} 条</span>
    </div>

    <div class="flex-1 overflow-y-auto border border-slate-800 rounded-xl">
      <table class="w-full text-xs">
        <thead class="bg-slate-900/80 sticky top-0 text-slate-400">
          <tr class="text-left">
            <th class="px-3 py-2 font-medium">文件名</th>
            <th class="px-3 py-2 font-medium">状态</th>
            <th class="px-3 py-2 font-medium">大小</th>
            <th class="px-3 py-2 font-medium">重试</th>
            <th class="px-3 py-2 font-medium">更新时间</th>
            <th class="px-3 py-2 font-medium">错误</th>
            <th class="px-3 py-2 font-medium">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="t in tasks"
            :key="t.taskId"
            class="border-t border-slate-800/80 hover:bg-slate-900/40"
          >
            <td class="px-3 py-2 text-slate-200 max-w-[220px] truncate" :title="t.originalName">
              {{ t.originalName }}
            </td>
            <td class="px-3 py-2">
              <span class="px-1.5 py-0.5 rounded text-[10px]" :class="statusClass(t.status)">
                {{ statusLabel(t.status) }}
              </span>
            </td>
            <td class="px-3 py-2 text-slate-400">{{ t.formattedSize }}</td>
            <td class="px-3 py-2 text-slate-400">{{ t.retryCount }}</td>
            <td class="px-3 py-2 text-slate-500">{{ formatTime(t.updatedAt) }}</td>
            <td class="px-3 py-2 text-red-400 max-w-[200px] truncate" :title="t.errorMessage || ''">
              {{ t.errorMessage || "—" }}
            </td>
            <td class="px-3 py-2 space-x-1">
              <button
                v-if="canRequeue(t.status)"
                class="px-2 py-0.5 rounded bg-emerald-700/40 text-emerald-300 hover:bg-emerald-600/50"
                @click="requeue(t.taskId)"
              >
                重试
              </button>
              <button
                v-if="canPause(t.status)"
                class="px-2 py-0.5 rounded bg-amber-700/40 text-amber-300 hover:bg-amber-600/50"
                @click="pause(t.taskId)"
              >
                暂停
              </button>
            </td>
          </tr>
          <tr v-if="!loading && tasks.length === 0">
            <td colspan="7" class="px-3 py-8 text-center text-slate-500">暂无任务</td>
          </tr>
        </tbody>
      </table>
    </div>

    <p v-if="message" class="text-xs" :class="messageOk ? 'text-emerald-400' : 'text-red-400'">
      {{ message }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { RefreshCw } from "lucide-vue-next";
import { listUploadTasks, requeueUploadTask, pauseUploadTask } from "../api/tauri";
import type { UploadTaskDto } from "../types";

const tasks = ref<UploadTaskDto[]>([]);
const statusFilter = ref("");
const loading = ref(false);
const message = ref("");
const messageOk = ref(true);

function statusLabel(s: string) {
  const map: Record<string, string> = {
    queued: "待上传",
    retryable_failed: "可重试失败",
    missing: "本地缺失",
    paused: "已暂停",
    backed_up: "已校验",
  };
  return map[s] || s;
}

function statusClass(s: string) {
  const map: Record<string, string> = {
    queued: "bg-slate-700 text-slate-200",
    retryable_failed: "bg-red-900/60 text-red-300",
    missing: "bg-orange-900/60 text-orange-300",
    paused: "bg-amber-900/60 text-amber-300",
    backed_up: "bg-emerald-900/60 text-emerald-300",
  };
  return map[s] || "bg-slate-700 text-slate-200";
}

function canRequeue(s: string) {
  return ["retryable_failed", "missing", "paused"].includes(s);
}

function canPause(s: string) {
  return s === "queued";
}

function formatTime(iso: string) {
  if (!iso) return "—";
  return iso.replace("T", " ").slice(0, 19);
}

async function refresh() {
  loading.value = true;
  try {
    tasks.value = await listUploadTasks(statusFilter.value || undefined, 300);
  } catch (err) {
    message.value = `加载失败: ${err}`;
    messageOk.value = false;
  } finally {
    loading.value = false;
  }
}

async function requeue(taskId: string) {
  try {
    await requeueUploadTask(taskId);
    message.value = "已重新入队";
    messageOk.value = true;
    await refresh();
  } catch (err) {
    message.value = `重试失败: ${err}`;
    messageOk.value = false;
  }
}

async function pause(taskId: string) {
  try {
    await pauseUploadTask(taskId);
    message.value = "已暂停";
    messageOk.value = true;
    await refresh();
  } catch (err) {
    message.value = `暂停失败: ${err}`;
    messageOk.value = false;
  }
}

onMounted(refresh);
</script>
