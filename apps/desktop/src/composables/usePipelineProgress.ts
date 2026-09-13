// 任务运行实时进度：订阅 Tauri 事件，供任务页状态条与运行历史刷新使用。
// keep-alive 下组件不会真正卸载，用 onActivated/onDeactivated 暂停与恢复订阅。
import { onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  RunFinishedEvent,
  RunItemEvent,
  RunProgressEvent,
  RunStageEvent,
} from "../types";

export interface LiveRunState {
  runId: string | null;
  stage: string;
  stageStatus: string;
  stageMessage: string;
  done: number;
  total: number;
  currentName: string;
  lastError: string;
}

const initial = (): LiveRunState => ({
  runId: null,
  stage: "",
  stageStatus: "",
  stageMessage: "",
  done: 0,
  total: 0,
  currentName: "",
  lastError: "",
});

/** 订阅运行进度事件；keep-alive 切页时暂停，卸载时清理。 */
export function usePipelineProgress(onFinished?: () => void) {
  const live = ref<LiveRunState>(initial());
  let unlisteners: UnlistenFn[] = [];
  let starting: Promise<void> | null = null;

  function reset() {
    live.value = initial();
  }

  async function startListening() {
    if (starting) return starting;
    if (unlisteners.length) return;
    starting = (async () => {
      const offs: UnlistenFn[] = [];
      offs.push(
        await listen<RunProgressEvent>("run://progress", (e) => {
          live.value = {
            ...live.value,
            runId: e.payload.runId,
            stage: e.payload.stage,
            done: e.payload.done,
            total: e.payload.total,
            currentName: e.payload.currentName || "",
          };
        }),
      );
      offs.push(
        await listen<RunStageEvent>("run://stage", (e) => {
          live.value = {
            ...live.value,
            runId: e.payload.runId,
            stage: e.payload.stage,
            stageStatus: e.payload.status,
            stageMessage: e.payload.message || "",
            done:
              e.payload.status === "running" && e.payload.stage === live.value.stage
                ? live.value.done
                : 0,
            total:
              e.payload.status === "running" && e.payload.stage === live.value.stage
                ? live.value.total
                : 0,
            currentName: e.payload.status === "running" ? "" : live.value.currentName,
          };
        }),
      );
      offs.push(
        await listen<RunItemEvent>("run://item", (e) => {
          if (e.payload.error) {
            live.value = {
              ...live.value,
              lastError: e.payload.error,
            };
          }
        }),
      );
      offs.push(
        await listen<RunFinishedEvent>("run://finished", () => {
          onFinished?.();
        }),
      );
      unlisteners = offs;
    })().finally(() => {
      starting = null;
    });
    return starting;
  }

  function stopListening() {
    for (const off of unlisteners) {
      try {
        off();
      } catch {
        /* ignore */
      }
    }
    unlisteners = [];
  }

  onMounted(() => {
    void startListening();
  });

  onActivated(() => {
    void startListening();
  });

  onDeactivated(() => {
    stopListening();
  });

  onBeforeUnmount(() => {
    stopListening();
  });

  return { live, reset };
}
