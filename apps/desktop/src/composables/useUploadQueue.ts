// 上传队列状态与操作：刷新列表、重新入队、暂停及删除本地缺失项。
import { ref } from "vue";
import {
  deleteMissingUploadTask,
  listUploadTasks,
  pauseUploadTask,
  requeueUploadTask,
} from "../api/tauri";
import type { UploadTaskDto } from "../types";
import { confirmAction } from "./useConfirm";
import { pushToast } from "./useToast";

/** 管理任务页队列；刷新只应用最新请求，防止轮询覆盖操作后的列表。 */
export function useUploadQueue() {
  const tasks = ref<UploadTaskDto[]>([]);
  const statusFilter = ref("pending");
  const pendingQueueCount = ref(0);
  const tasksLoading = ref(false);
  const busyTaskIds = ref(new Set<string>());
  let refreshVersion = 0;
  let lastErrorToastAt = 0;

  /** 按当前筛选刷新列表与待处理角标；静默失败最多每分钟提示一次。 */
  async function refreshTasks(silent = false) {
    const version = ++refreshVersion;
    const filter = statusFilter.value;
    if (!silent) tasksLoading.value = true;
    try {
      const [list, pending] = await Promise.all([
        listUploadTasks(filter || undefined, 300),
        filter === "pending" || filter === "" ? Promise.resolve(null) : listUploadTasks("pending", 300),
      ]);
      if (version !== refreshVersion) return;
      tasks.value = list;
      pendingQueueCount.value = (pending ?? list).filter((task) =>
        ["queued", "retryable_failed", "missing", "paused"].includes(task.status),
      ).length;
    } catch (err) {
      if (version !== refreshVersion) return;
      const now = Date.now();
      if (!silent || now - lastErrorToastAt > 60000) {
        lastErrorToastAt = now;
        pushToast({
          tone: silent ? "warning" : "danger",
          title: silent ? "上传队列刷新失败" : "加载上传队列失败",
          description: String(err),
        });
      }
    } finally {
      if (!silent) tasksLoading.value = false;
    }
  }

  /** 防止同一任务重复提交；无论成功或状态冲突，结束后均重新读取队列。 */
  async function runAction(taskId: string, action: () => Promise<void>, failureTitle: string) {
    if (busyTaskIds.value.has(taskId)) return;
    busyTaskIds.value.add(taskId);
    try {
      await action();
    } catch (err) {
      pushToast({ tone: "danger", title: failureTitle, description: String(err) });
    } finally {
      await refreshTasks();
      busyTaskIds.value.delete(taskId);
    }
  }

  /** 将现有任务重新入队，等待下次运行上传。 */
  async function requeue(taskId: string) {
    await runAction(taskId, async () => {
      await requeueUploadTask(taskId);
      pushToast({ tone: "success", title: "已重新入队，下次运行时上传" });
    }, "重试失败");
  }

  /** 暂停待上传任务并刷新状态。 */
  async function pause(taskId: string) {
    await runAction(taskId, async () => {
      await pauseUploadTask(taskId);
      pushToast({ tone: "success", title: "已暂停" });
    }, "暂停失败");
  }

  /** 确认后删除单个缺失项；后端以最新状态决定是否允许删除。 */
  async function deleteMissingTask(task: UploadTaskDto) {
    if (task.status !== "missing") return;
    await runAction(task.taskId, async () => {
      const ok = await confirmAction({
        title: "删除上传队列项？",
        description: `将从上传队列删除「${task.originalName}」。文件库记录、原文件、缓存、远端归档和运行历史均保留。`,
        confirmLabel: "删除队列项",
        danger: true,
      });
      if (!ok) return;
      await deleteMissingUploadTask(task.taskId);
      tasks.value = tasks.value.filter((item) => item.taskId !== task.taskId);
      pushToast({ tone: "success", title: "已删除上传队列项" });
    }, "删除上传队列项失败");
  }

  return {
    tasks, statusFilter, pendingQueueCount, tasksLoading, busyTaskIds,
    refreshTasks, requeue, pause, deleteMissingTask,
  };
}
