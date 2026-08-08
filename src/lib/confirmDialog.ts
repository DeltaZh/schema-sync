import { ref } from "vue";

interface ConfirmState {
  open: boolean;
  title: string;
  message: string;
  confirmText: string;
  cancelText: string;
  danger: boolean;
  /** 非空时须原样输入该文本才能确认 */
  requireTypedText: string;
  typed: string;
  resolve: ((ok: boolean) => void) | null;
}

const state = ref<ConfirmState>({
  open: false,
  title: "请确认",
  message: "",
  confirmText: "确定",
  cancelText: "取消",
  danger: false,
  requireTypedText: "",
  typed: "",
  resolve: null,
});

export function confirmDialogState() {
  return state;
}

export type ConfirmOptions = {
  title?: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
  requireTypedText?: string;
};

/** macOS Tauri 下 window.confirm 会静默返回 false，统一走应用内弹窗 */
export function askConfirm(
  message: string,
  options?: ConfirmOptions,
): Promise<boolean> {
  return new Promise((resolve) => {
    if (state.value.resolve) {
      state.value.resolve(false);
    }
    state.value = {
      open: true,
      title: options?.title ?? "请确认",
      message,
      confirmText: options?.confirmText ?? "确定",
      cancelText: options?.cancelText ?? "取消",
      danger: options?.danger ?? false,
      requireTypedText: options?.requireTypedText ?? "",
      typed: "",
      resolve,
    };
  });
}

export function setConfirmTyped(value: string) {
  state.value = { ...state.value, typed: value };
}

export function canSubmitConfirm(): boolean {
  const s = state.value;
  if (!s.requireTypedText) return true;
  return s.typed === s.requireTypedText;
}

export function answerConfirm(ok: boolean) {
  if (ok && !canSubmitConfirm()) return;
  const resolve = state.value.resolve;
  state.value = {
    ...state.value,
    open: false,
    typed: "",
    resolve: null,
  };
  resolve?.(ok);
}

/** 高风险执行：先说明，再要求输入「确认执行」 */
export async function askDangerousExecute(summary: string): Promise<boolean> {
  const first = await askConfirm(
    `${summary}\n\n其中包含高风险操作（如写入/更新/删除数据、删表、删字段等）。`,
    {
      title: "高风险操作确认",
      confirmText: "继续",
      danger: true,
    },
  );
  if (!first) return false;
  return askConfirm(
    "这是第二次确认。请在下方输入「确认执行」（须完全一致）后继续。",
    {
      title: "二次确认",
      confirmText: "确认执行",
      danger: true,
      requireTypedText: "确认执行",
    },
  );
}
