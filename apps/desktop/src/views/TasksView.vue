<!--
  ChatVault 任务中心
  职责：上传/校验任务状态轮询、重试与暂停。
-->
<template>
  <div class="flex h-full flex-col gap-4 p-6">
    <div class="flex flex-wrap items-end justify-between gap-3">
      <div>
        <h2 class="text-cv-page text-cv-text">任务</h2>
        <p class="mt-0.5 text-cv-caption text-cv-text-2">归档队列状态；每 5 秒自动刷新</p>
      </div>
      <div class="flex items-center gap-2">
        <div class="w-40 shrink-0">
          <UiSelect v-model="statusFilter" @change="() => refresh()">
            <option value="">全部状态</option>
            <option value="queued">待上传</option>
            <option value="retryable_failed">可重试失败</option>
            <option value="missing">本地缺失</option>
            <option value="paused">已暂停</option>
            <option value="backed_up">已校验归档</option>
          </UiSelect>
        </div>
        <UiButton size="sm" variant="secondary" :loading="loading" @click="() => refresh()">刷新</UiButton>
      </div>
    </div>

    <p class="text-cv-caption text-cv-text-3">共 {{ tasks.length }} 条（最多显示 300）</p>

    <div class="min-h-0 flex-1 overflow-auto rounded-cv-lg border border-cv-border bg-cv-surface">
      <table class="w-full text-left text-cv-body">
        <thead class="sticky top-0 border-b border-cv-border bg-cv-surface-2 text-cv-caption text-cv-text-2">
          <tr>
            <th class="px-3 py-2.5 font-medium">文件名</th>
            <th class="w-28 px-3 py-2.5 font-medium">状态</th>
            <th class="w-20 px-3 py-2.5 font-medium">大小</th>
            <th class="w-16 px-3 py-2.5 font-medium">重试</th>
            <th class="w-40 px-3 py-2.5 font-medium">更新时间</th>
            <th class="px-3 py-2.5 font-medium">错误</th>
            <th class="w-28 px-3 py-2.5 font-medium">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="t in tasks"
            :key="t.taskId"
            class="border-b border-cv-border/60 last:border-0 hover:bg-cv-surface-2"
          >
            <td class="max-w-[240px] truncate px-3 py-2 text-cv-text" :title="t.originalName">
              {{ t.originalName }}
            </td>
            <td class="px-3 py-2">
              <UiBadge :tone="statusTone(t.status)">{{ statusLabel(t.status) }}</UiBadge>
            </td>
            <td class="px-3 py-2 text-cv-text-2">{{ t.formattedSize }}</td>
            <td class="px-3 py-2 text-cv-text-2">{{ t.retryCount }}</td>
            <td class="px-3 py-2 text-cv-text-3">{{ formatTime(t.updatedAt) }}</td>
            <td class="max-w-[220px] truncate px-3 py-2" :class="t.errorMessage ? 'text-cv-danger' : 'text-cv-text-3'" :title="t.errorMessage || ''">
              {{ t.errorMessage || "—" }}
            </td>
            <td class="px-3 py-2">
              <div class="flex gap-1">
                <UiButton v-if="canRequeue(t.status)" size="sm" variant="secondary" @click="requeue(t.taskId)">
                  重试
                </UiButton>
                <UiButton v-if="canPause(t.status)" size="sm" variant="ghost" @click="pause(t.taskId)">
                  暂停
                </UiButton>
              </div>
            </td>
          </tr>
          <tr v-if="!loading && tasks.length === 0">
            <td colspan="7" class="py-12 text-center text-cv-caption text-cv-text-3">暂无任务</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from "vue";
import UiButton from "../components/ui/UiButton.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import UiBadge from "../components/ui/UiBadge.vue";
import { listUploadTasks, requeueUploadTask, pauseUploadTask } from "../api/tauri";
import { pushToast } from "../composables/useToast";
import type { UploadTaskDto } from "../types";

const tasks = ref<UploadTaskDto[]>([]);
const statusFilter = ref("");
const loading = ref(false);
let pollTimer: number | undefined;

const statusMap: Record<string, string> = {
  queued: "待上传",
  retryable_failed: "可重试失败",
  missing: "本地缺失",
  paused: "已暂停",
  backed_up: "已校验",
};

const toneMap: Record<string, "neutral" | "accent" | "success" | "warning" | "danger"> = {
  queued: "neutral",
  retryable_failed: "danger",
  missing: "warning",
  paused: "warning",
  backed_up: "success",
};

function statusLabel(s: string) {
  return statusMap[s] || s;
}

function statusTone(s: string) {
  return toneMap[s] || "neutral";
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

let inflight = false;
let lastErrorToastAt = 0;

/**
 * 拉取任务列表。
 * silent=true 时用于轮询：失败不弹 toast，且带在途守卫防重入。
 */
async function refresh(silent = false) {
  if (inflight) return;
  inflight = true;
  if (!silent) loading.value = true;
  try {
    tasks.value = await listUploadTasks(statusFilter.value || undefined, 300);
  } catch (err) {
    if (!silent) {
      pushToast({ tone: "danger", title: "加载任务失败", description: String(err) });
    } else {
      const now = Date.now();
      if (now - lastErrorToastAt > 60000) {
        lastErrorToastAt = now;
        pushToast({ tone: "warning", title: "任务列表刷新失败", description: String(err) });
      }
    }
  } finally {
    inflight = false;
    if (!silent) loading.value = false;
  }
}

/** 重新入队。 */
async function requeue(taskId: string) {
  try {
    await requeueUploadTask(taskId);
    pushToast({ tone: "success", title: "已重新入队" });
    await refresh();
  } catch (err) {
    pushToast({ tone: "danger", title: "重试失败", description: String(err) });
  }
}

/** 暂停任务。 */
async function pause(taskId: string) {
  try {
    await pauseUploadTask(taskId);
    pushToast({ tone: "success", title: "已暂停" });
    await refresh();
  } catch (err) {
    pushToast({ tone: "danger", title: "暂停失败", description: String(err) });
  }
}

onMounted(() => {
  void refresh();
  pollTimer = window.setInterval(() => {
    void refresh(true);
  }, 5000);
});

onBeforeUnmount(() => {
  if (pollTimer) window.clearInterval(pollTimer);
});
</script>
