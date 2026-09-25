<script setup lang="ts">
import { dismiss, toasts, type ToastKind } from "../utils/toast";

const palette: Record<ToastKind, { bar: string; icon: string }> = {
  success: { bar: "#a9bd6b", icon: "M2 6.2l2.6 2.6L10 .8" },
  error: { bar: "#dd5f4a", icon: "M1.5 1.5l9 9M10.5 1.5l-9 9" },
  info: { bar: "#e0913c", icon: "M6 3v.01M6 5.5v4" },
  warn: { bar: "#e8b04b", icon: "M6 2.5v4.2M6 9v.01" },
};
</script>

<template>
  <Teleport to="body">
    <div class="pointer-events-none fixed right-5 bottom-5 z-[100] flex flex-col items-end gap-2">
      <TransitionGroup name="toast">
        <div
          v-for="item in toasts"
          :key="item.id"
          class="pointer-events-auto flex w-[330px] items-start gap-3 overflow-hidden rounded-xl border border-line bg-surface-2 py-3 pr-3.5 pl-3.5 shadow-xl"
          @click="dismiss(item.id)"
        >
          <span
            class="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full"
            :style="{ background: palette[item.kind].bar }"
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path
                :d="palette[item.kind].icon"
                stroke="#100d0a"
                stroke-width="1.8"
                stroke-linecap="round"
              />
            </svg>
          </span>
          <p class="flex-1 text-[14px] leading-snug text-ink break-words">{{ item.message }}</p>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.24s cubic-bezier(0.22, 1, 0.36, 1);
}
.toast-enter-from {
  opacity: 0;
  transform: translateX(16px) scale(0.97);
}
.toast-leave-to {
  opacity: 0;
  transform: translateX(16px);
}
</style>
