<script setup lang="ts">
import { answerConfirm, confirmState } from "../composables/useConfirm";

function close(value: boolean) {
  answerConfirm(value);
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="confirmState.open"
        class="fixed inset-0 z-[90] flex items-center justify-center bg-black/60 backdrop-blur-[2px]"
        @click.self="close(false)"
      >
        <div class="anim-pop w-[400px] rounded-2xl border border-line bg-surface p-5 shadow-2xl">
          <h3 class="text-[15px] font-semibold text-ink">{{ confirmState.title }}</h3>
          <p class="mt-2.5 text-[13px] leading-relaxed text-ink-2 whitespace-pre-line">
            {{ confirmState.message }}
          </p>
          <div class="mt-5 flex justify-end gap-2">
            <button class="btn btn-ghost" @click="close(false)">
              {{ confirmState.cancelText }}
            </button>
            <button
              class="btn"
              :class="confirmState.danger ? 'btn-danger' : 'btn-primary'"
              @click="close(true)"
            >
              {{ confirmState.confirmText }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.16s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
