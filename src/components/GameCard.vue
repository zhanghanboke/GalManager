<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Game } from "../api";
import { STATUS_COLORS, STATUS_LABELS, formatDuration } from "../utils/format";

const props = defineProps<{
  game: Game;
  running?: boolean;
  selectionMode?: boolean;
  selected?: boolean;
  index?: number;
  /**
   * 键盘导航（roving tabindex）：整个网格里只有当前项 tabindex=0，
   * 其余为 -1，这样 Tab 只需进出一趟，方向键负责在卡片间移动。
   */
  focused?: boolean;
}>();

const emit = defineEmits<{
  open: [game: Game];
  launch: [game: Game];
  select: [game: Game];
  focused: [index: number];
  contextmenu: [payload: { game: Game; x: number; y: number }];
}>();

const coverUrl = ref<string | null>(null);
const imageFailed = ref(false);

watch(
  () => props.game.coverPath,
  (path) => {
    imageFailed.value = false;
    coverUrl.value = path ? convertFileSrc(path) : null;
  },
  { immediate: true },
);

const statusColor = computed(() => STATUS_COLORS[props.game.playStatus] ?? "#8a7d6d");
const statusLabel = computed(() => STATUS_LABELS[props.game.playStatus] ?? props.game.playStatus);

/** 无封面时的占位渐变色：由标题哈希决定，保证同一游戏颜色稳定（暖色系） */
const placeholder = computed(() => {
  const palettes = [
    ["#3a2a1e", "#52381f"],
    ["#402a20", "#5c3a24"],
    ["#33261c", "#4a3524"],
    ["#452c1c", "#63401f"],
    ["#2e2118", "#443020"],
  ];
  let hash = 0;
  for (let i = 0; i < props.game.title.length; i += 1) {
    hash = (hash * 31 + props.game.title.charCodeAt(i)) >>> 0;
  }
  const [from, to] = palettes[hash % palettes.length];
  return `linear-gradient(145deg, ${from}, ${to})`;
});

const initial = computed(() => props.game.title.trim().charAt(0) || "?");

function onContextMenu(event: MouseEvent) {
  event.preventDefault();
  emit("contextmenu", { game: props.game, x: event.clientX, y: event.clientY });
}
</script>

<template>
  <article
    data-game-card
    :tabindex="focused ? 0 : -1"
    :aria-label="game.title"
    class="group cover-enter relative cursor-pointer select-none rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-accent/70"
    :style="{ animationDelay: `${Math.min((index ?? 0) * 14, 260)}ms` }"
    @click="selectionMode ? emit('select', game) : emit('open', game)"
    @focus="emit('focused', index ?? 0)"
    @contextmenu="onContextMenu"
  >
    <!-- 封面 -->
    <div
      class="relative overflow-hidden rounded-xl border bg-surface-2 transition duration-200 group-hover:-translate-y-1 group-hover:border-accent/45 group-hover:shadow-2xl group-hover:shadow-accent/10"
      :class="selected ? 'border-accent ring-2 ring-accent/40' : 'border-line-soft'"
      style="aspect-ratio: 3 / 4"
    >
      <img
        v-if="coverUrl && !imageFailed"
        :src="coverUrl"
        :alt="game.title"
        class="h-full w-full object-cover transition duration-300 group-hover:scale-[1.04]"
        loading="lazy"
        draggable="false"
        @error="imageFailed = true"
      />
      <div
        v-else
        class="flex h-full w-full items-center justify-center"
        :style="{ background: placeholder }"
      >
        <span class="text-[46px] font-semibold text-white/25">{{ initial }}</span>
      </div>

      <!-- 顶部角标 -->
      <div class="absolute top-2 left-2 flex items-center gap-1.5">
        <span
          v-if="game.favorite === 1"
          class="flex h-5 w-5 items-center justify-center rounded-md bg-black/55 backdrop-blur-sm"
          title="已收藏"
        >
          <svg width="11" height="11" viewBox="0 0 12 12" fill="#e8b04b">
            <path d="M6 .8l1.6 3.3 3.6.5-2.6 2.5.6 3.6L6 9l-3.2 1.7.6-3.6L.8 4.6l3.6-.5z" />
          </svg>
        </span>
        <span
          v-if="running"
          class="flex h-5 items-center gap-1 rounded-md bg-black/60 px-1.5 backdrop-blur-sm"
          title="正在游玩"
        >
          <span class="h-1.5 w-1.5 rounded-full bg-sage" />
          <span class="text-[11px] font-medium text-sage">运行中</span>
        </span>
      </div>

      <!-- 选中态 -->
      <div
        v-if="selectionMode"
        class="absolute top-2 right-2 flex h-5 w-5 items-center justify-center rounded-md border transition"
        :class="selected ? 'border-accent bg-accent' : 'border-white/35 bg-black/40'"
      >
        <svg v-if="selected" width="11" height="11" viewBox="0 0 12 12" fill="none">
          <path d="M2.5 6.2l2.4 2.4L9.5 3.6" stroke="#fff" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </div>

      <!-- 底部状态条 -->
      <div class="absolute inset-x-0 bottom-0 flex items-center gap-1.5 bg-gradient-to-t from-black/80 to-transparent px-2 pt-6 pb-1.5">
        <span class="h-1.5 w-1.5 shrink-0 rounded-full" :style="{ background: statusColor }" />
        <span class="text-[12px] text-white/75">{{ statusLabel }}</span>
        <span v-if="game.totalPlaySeconds > 0" class="ml-auto text-[12px] text-white/60">
          {{ formatDuration(game.totalPlaySeconds) }}
        </span>
      </div>

      <!-- 悬停操作层 -->
      <div
        v-if="!selectionMode"
        class="absolute inset-0 flex items-center justify-center bg-black/45 opacity-0 backdrop-blur-[1px] transition duration-200 group-hover:opacity-100"
      >
        <button
          class="flex h-11 w-11 items-center justify-center rounded-full bg-accent text-white shadow-lg transition hover:scale-110 hover:bg-accent-hi"
          :title="running ? '停止游戏' : '启动游戏'"
          @click.stop="emit('launch', game)"
        >
          <svg v-if="running" width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
            <rect x="2.5" y="2.5" width="9" height="9" rx="1.6" />
          </svg>
          <svg v-else width="15" height="15" viewBox="0 0 14 14" fill="currentColor">
            <path d="M3.6 2.1l8 4.9-8 4.9z" />
          </svg>
        </button>
      </div>
    </div>

    <!-- 标题 -->
    <div class="mt-2 px-0.5">
      <h3 class="line-clamp-1 text-[14px] font-medium text-ink" :title="game.title">
        {{ game.title }}
      </h3>
      <p v-if="game.engine" class="mt-0.5 line-clamp-1 text-[12px] text-ink-3">
        {{ game.developer || game.engine }}
      </p>
    </div>
  </article>
</template>
