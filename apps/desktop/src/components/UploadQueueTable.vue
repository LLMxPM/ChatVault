<!-- 上传队列表格：展示状态、异常原因与单项操作。 -->
<template>
  <div class="flex min-h-0 flex-1 flex-col p-4 pt-2">
    <p class="mb-2 shrink-0 text-cv-caption text-cv-text-3">
      共 {{ tasks.length }} 条
      <template v-if="statusFilter === 'pending'"> · 仅显示待处理（待上传 / 失败 / 缺失 / 暂停）</template>
    </p>
    <div class="min-h-0 flex-1 overflow-auto rounded-cv border border-cv-border">
      <table class="w-full table-fixed text-left text-cv-caption">
        <colgroup>
          <col />
          <col style="width: 12%" />
          <col style="width: 12%" />
          <col style="width: 12%" />
          <col style="width: 12%" />
          <col style="width: 11rem" />
        </colgroup>
        <thead class="sticky top-0 border-b border-cv-border bg-cv-surface-2 text-cv-text-2">
          <tr>
            <th class="px-2.5 py-2 font-medium">文件名</th>
            <th class="px-2.5 py-2 font-medium">状态</th>
            <th class="px-2.5 py-2 font-medium whitespace-nowrap">大小</th>
            <th class="px-2.5 py-2 font-medium whitespace-nowrap">更新时间</th>
            <th class="px-2.5 py-2 font-medium">说明</th>
            <th class="px-2.5 py-2 font-medium">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="t in tasks"
            :key="t.taskId"
            class="border-b border-cv-border/60 last:border-0"
          >
            <td class="truncate px-2.5 py-1.5 text-cv-text" :title="t.originalName">
              {{ t.originalName }}
            </td>
            <td class="px-2.5 py-1.5">
              <UiBadge :tone="statusTone(t.status)">{{ statusLabel(t.status) }}</UiBadge>
            </td>
            <td class="px-2.5 py-1.5 whitespace-nowrap text-cv-text-2">{{ t.formattedSize }}</td>
            <td class="px-2.5 py-1.5 whitespace-nowrap text-cv-text-3">
              {{ formatDateTime(t.updatedAt, { compact: true }) }}
            </td>
            <td
              class="truncate px-2.5 py-1.5 text-cv-text-3"
              :title="queueNoteTitle(t)"
            >
              {{ queueNote(t) }}
            </td>
            <td class="px-2.5 py-1.5">
              <div class="flex flex-wrap gap-1 whitespace-nowrap">
                <UiButton
                  v-if="canRequeue(t.status)"
                  size="sm"
                  variant="secondary"
                  :disabled="busyTaskIds.has(t.taskId)"
                  @click="emit('requeue', t.taskId)"
                >
                  重新入队
                </UiButton>
                <UiButton
                  v-if="canPause(t.status)"
                  size="sm"
                  variant="ghost"
                  :disabled="busyTaskIds.has(t.taskId)"
                  @click="emit('pause', t.taskId)"
                >
                  暂停
                </UiButton>
                <UiButton
                  v-if="t.status === 'missing'"
                  size="sm"
                  variant="danger"
                  :disabled="busyTaskIds.has(t.taskId)"
                  @click="emit('delete', t)"
                >
                  删除
                </UiButton>
              </div>
            </td>
          </tr>
          <tr v-if="!tasksLoading && tasks.length === 0">
            <td colspan="6" class="py-8 text-center text-cv-text-3">
              {{ statusFilter === "pending" ? "没有待处理的上传项" : "暂无上传项" }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import UiButton from "./ui/UiButton.vue";
import UiBadge from "./ui/UiBadge.vue";
import type { UploadTaskDto } from "../types";
import { formatDateTime } from "../utils/format";

defineProps<{
  tasks: UploadTaskDto[];
  statusFilter: string;
  tasksLoading: boolean;
  busyTaskIds: Set<string>;
}>();

const emit = defineEmits<{
  requeue: [taskId: string];
  pause: [taskId: string];
  delete: [task: UploadTaskDto];
}>();

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

/** 将持久状态映射为用户可读标签。 */
function statusLabel(s: string) {
  return statusMap[s] || s;
}

/** 按任务状态选择徽标颜色。 */
function statusTone(s: string) {
  return toneMap[s] || "neutral";
}

/** 失败、缺失和暂停项允许用户重新入队。 */
function canRequeue(s: string) {
  return ["retryable_failed", "missing", "paused"].includes(s);
}

/** 只为待上传项展示暂停入口。 */
function canPause(s: string) {
  return s === "queued";
}

/** 队列行说明：失败原因、重试次数或占位。 */
function queueNote(t: UploadTaskDto) {
  if (t.status === "retryable_failed" || t.status === "missing") {
    if (t.retryCount > 0) return `已试 ${t.retryCount} 次`;
    return t.errorMessage ? "见详情" : "—";
  }
  if (t.status === "paused" && t.retryCount > 0) return `已试 ${t.retryCount} 次`;
  return "—";
}

/** 悬浮时展示完整原因与重试次数，没有错误时展示源路径。 */
function queueNoteTitle(t: UploadTaskDto) {
  const parts: string[] = [];
  if (t.errorMessage) parts.push(t.errorMessage);
  if (t.retryCount > 0) parts.push(`重试 ${t.retryCount} 次`);
  return parts.join(" · ") || t.originalPath || "";
}

</script>
