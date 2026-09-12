// 破坏性操作确认对话框：Promise 化，替换原生 confirm/alert
import { ref } from "vue";

export interface ConfirmOptions {
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

interface ConfirmState extends ConfirmOptions {
  open: boolean;
}

const state = ref<ConfirmState>({ open: false, title: "" });
let resolver: ((ok: boolean) => void) | null = null;

/** 弹出确认框，用户确认返回 true。 */
export function confirmAction(options: ConfirmOptions): Promise<boolean> {
  if (resolver) {
    resolver(false);
    resolver = null;
  }
  state.value = { ...options, open: true };
  return new Promise<boolean>((resolve) => {
    resolver = resolve;
  });
}

/** 关闭并返回结果。 */
export function settleConfirm(ok: boolean): void {
  state.value = { ...state.value, open: false };
  if (resolver) {
    resolver(ok);
    resolver = null;
  }
}

export function useConfirm() {
  return { confirmState: state, confirmAction, settleConfirm };
}
