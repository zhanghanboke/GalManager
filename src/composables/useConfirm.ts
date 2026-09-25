/** 全局确认对话框（替代 window.confirm，保持视觉一致） */

import { reactive } from "vue";

export interface ConfirmOptions {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
}

interface ConfirmState extends ConfirmOptions {
  open: boolean;
  resolve: ((value: boolean) => void) | null;
}

export const confirmState = reactive<ConfirmState>({
  open: false,
  title: "",
  message: "",
  confirmText: "确定",
  cancelText: "取消",
  danger: false,
  resolve: null,
});

export function confirmDialog(options: ConfirmOptions): Promise<boolean> {
  confirmState.open = true;
  confirmState.title = options.title;
  confirmState.message = options.message;
  confirmState.confirmText = options.confirmText ?? "确定";
  confirmState.cancelText = options.cancelText ?? "取消";
  confirmState.danger = options.danger ?? false;

  return new Promise<boolean>((resolve) => {
    confirmState.resolve = resolve;
  });
}

export function answerConfirm(value: boolean) {
  confirmState.resolve?.(value);
  confirmState.resolve = null;
  confirmState.open = false;
}
