// 全局轻提示：替代 window.alert，支持可选动作按钮
import { ref } from "vue";

export type ToastTone = "info" | "success" | "warning" | "danger";

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastItem {
  id: number;
  tone: ToastTone;
  title: string;
  description?: string;
  actions?: ToastAction[];
}

const toasts = ref<ToastItem[]>([]);
let nextId = 1;
const DEFAULT_MS = 3200;

/** 追加一条 toast；带 actions 时延长展示时间。 */
export function pushToast(
  input: {
    tone?: ToastTone;
    title: string;
    description?: string;
    actions?: ToastAction[];
    durationMs?: number;
  },
): void {
  const actions = input.actions?.length ? input.actions : undefined;
  const item: ToastItem = {
    id: nextId++,
    tone: input.tone ?? "info",
    title: input.title,
    description: input.description,
    actions,
  };
  const next = [...toasts.value, item];
  toasts.value = next.length > 5 ? next.slice(next.length - 5) : next;
  const ms = input.durationMs ?? (actions ? 6000 : DEFAULT_MS);
  window.setTimeout(() => dismissToast(item.id), ms);
}

/** 关闭指定 toast。 */
export function dismissToast(id: number): void {
  toasts.value = toasts.value.filter((t) => t.id !== id);
}

export function useToast() {
  return { toasts, pushToast, dismissToast };
}
