/** 轻量级全局提示（Toast） */

import { reactive } from "vue";

export type ToastKind = "success" | "error" | "info" | "warn";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

let seed = 0;

export const toasts = reactive<ToastItem[]>([]);

function push(kind: ToastKind, message: string, duration = 3200) {
  const id = ++seed;
  toasts.push({ id, kind, message });
  window.setTimeout(() => dismiss(id), duration);
}

export function dismiss(id: number) {
  const index = toasts.findIndex((item) => item.id === id);
  if (index >= 0) toasts.splice(index, 1);
}

export const toast = {
  success: (message: string) => push("success", message),
  error: (message: string) => push("error", message, 5000),
  info: (message: string) => push("info", message),
  warn: (message: string) => push("warn", message, 4200),
};

/** 把后端抛出的错误统一转成可读文本 */
export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
