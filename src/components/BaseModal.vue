<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";

const props = withDefaults(
  defineProps<{
    title: string;
    subtitle?: string;
    width?: string;
    /** 点击遮罩是否关闭 */
    maskClosable?: boolean;
  }>(),
  { width: "640px", maskClosable: true },
);

const emit = defineEmits<{ close: [] }>();

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}

onMounted(() => window.addEventListener("keydown", onKeydown));
onUnmounted(() => window.removeEventListener("keydown", onKeydown));
</script>

<template>
  <Teleport to="body">
    <div
      class="fixed inset-0 z-[80] flex items-center justify-center bg-black/62 p-6 backdrop-blur-[2px]"
      @click.self="props.maskClosable && emit('close')"
    >
      <div
        class="anim-pop flex max-h-[86vh] w-full flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-2xl"
        :style="{ maxWidth: props.width }"
      >
        <header
          class="flex shrink-0 items-start justify-between gap-4 border-b border-line-soft px-5 py-4"
        >
          <div class="min-w-0">
            <h2 class="truncate text-[16.5px] font-semibold text-ink">{{ props.title }}</h2>
            <p v-if="props.subtitle" class="mt-0.5 truncate text-[13.5px] text-ink-3">
              {{ props.subtitle }}
            </p>
          </div>
          <button
            class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-ink-3 transition hover:bg-surface-3 hover:text-ink"
            title="关闭"
            @click="emit('close')"
          >
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
              <path
                d="M1 1l12 12M13 1L1 13"
                stroke="currentColor"
                stroke-width="1.7"
                stroke-linecap="round"
              />
            </svg>
          </button>
        </header>

        <div class="min-h-0 flex-1 scroll-y px-5 py-4">
          <slot />
        </div>

        <footer
          v-if="$slots.footer"
          class="flex shrink-0 items-center justify-end gap-2 border-t border-line-soft bg-surface-2/50 px-5 py-3.5"
        >
          <slot name="footer" />
        </footer>
      </div>
    </div>
  </Teleport>
</template>
