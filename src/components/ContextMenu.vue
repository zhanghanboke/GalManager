<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import type { Game } from "../api";

const props = defineProps<{
  game: Game;
  x: number;
  y: number;
  running?: boolean;
}>();

const emit = defineEmits<{
  close: [];
  openDetail: [];
  launch: [];
  edit: [];
  remove: [];
  openFolder: [];
}>();

const menuRef = ref<HTMLElement | null>(null);
const position = ref({ left: props.x, top: props.y });

const items = computed(() => [
  { key: "openDetail", label: "查看详情", icon: "eye" },
  { key: "launch", label: props.running ? "停止游戏" : "启动游戏", icon: "play" },
  { key: "openFolder", label: "打开游戏目录", icon: "folder" },
  { divider: true },
  { key: "edit", label: "编辑信息", icon: "pen" },
  { key: "remove", label: "从库中移除", icon: "trash", danger: true },
]);

function handle(key: string) {
  const map: Record<string, () => void> = {
    openDetail: () => emit("openDetail"),
    launch: () => emit("launch"),
    openFolder: () => emit("openFolder"),
    edit: () => emit("edit"),
    remove: () => emit("remove"),
  };
  map[key]?.();
  emit("close");
}

onMounted(() => {
  // 贴边修正，避免菜单溢出窗口
  const height = menuRef.value?.offsetHeight ?? 220;
  const width = menuRef.value?.offsetWidth ?? 180;
  position.value = {
    left: Math.min(props.x, window.innerWidth - width - 8),
    top: Math.min(props.y, window.innerHeight - height - 8),
  };
});
</script>

<template>
  <Teleport to="body">
    <div
      ref="menuRef"
      class="anim-pop fixed z-[95] w-[184px] overflow-hidden rounded-xl border border-line bg-surface-2 py-1.5 shadow-2xl"
      :style="{ left: `${position.left}px`, top: `${position.top}px` }"
      @click.stop
    >
      <p class="truncate px-3 pt-1 pb-2 text-[11.5px] font-medium text-ink-3">
        {{ game.title }}
      </p>
      <template v-for="(item, index) in items" :key="index">
        <div v-if="item.divider" class="my-1 h-px bg-line-soft" />
        <button
          v-else
          class="flex h-8 w-full items-center gap-2.5 px-3 text-[12.5px] transition"
          :class="
            item.danger
              ? 'text-danger hover:bg-danger/12'
              : 'text-ink-2 hover:bg-surface-3 hover:text-ink'
          "
          @click="handle(item.key!)"
        >
          <svg width="13" height="13" viewBox="0 0 14 14" fill="none" class="shrink-0">
            <template v-if="item.icon === 'eye'">
              <path d="M1 7s2.3-3.8 6-3.8S13 7 13 7s-2.3 3.8-6 3.8S1 7 1 7z" stroke="currentColor" stroke-width="1.3" />
              <circle cx="7" cy="7" r="1.6" stroke="currentColor" stroke-width="1.3" />
            </template>
            <template v-else-if="item.icon === 'play'">
              <path d="M4 2.4l7.4 4.6L4 11.6z" fill="currentColor" />
            </template>
            <template v-else-if="item.icon === 'folder'">
              <path d="M1.5 4a1.5 1.5 0 011.5-1.5h2.4l1.2 1.5H11A1.5 1.5 0 0112.5 5.5v4A1.5 1.5 0 0111 11H3a1.5 1.5 0 01-1.5-1.5z" stroke="currentColor" stroke-width="1.3" />
            </template>
            <template v-else-if="item.icon === 'pen'">
              <path d="M9.4 1.8l2.8 2.8-7 7L2 12.4l.8-3.2z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
            </template>
            <template v-else>
              <path d="M2 3.5h10M5.2 3.5V2.2h3.6v1.3M3.2 3.5l.7 8.2h6.2l.7-8.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
            </template>
          </svg>
          {{ item.label }}
        </button>
      </template>
    </div>
  </Teleport>
</template>
