<script setup lang="ts">
/**
 * 游戏库的「列表视图」：紧凑表格，一屏可看几十条，适合几百个游戏的库。
 *
 * 与封面墙共用同一份筛选 / 排序状态（排序仍走后端），只是呈现方式不同。
 */
import { computed } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Game } from "../api";
import {
  STATUS_COLORS,
  STATUS_LABELS,
  formatDate,
  formatDuration,
  formatRelative,
} from "../utils/format";

const props = defineProps<{
  games: Game[];
  runningIds: Set<number>;
  selectionMode: boolean;
  selectedIds: number[];
  sortBy: string;
  sortDesc: boolean;
  /** 键盘导航：当前聚焦的行下标（roving tabindex） */
  focusedIndex?: number;
}>();

const emit = defineEmits<{
  (e: "open", game: Game): void;
  (e: "launch", game: Game): void;
  (e: "select", id: number): void;
  (e: "focused", index: number): void;
  (e: "contextmenu", payload: { game: Game; x: number; y: number }): void;
  (e: "sort", value: string): void;
}>();

interface Column {
  key: string;
  label: string;
  /** 对应后端 sortBy；为空表示该列不可排序 */
  sort?: string;
  align?: "left" | "right";
  width?: string;
}

const columns: Column[] = [
  { key: "title", label: "名称", sort: "title" },
  { key: "status", label: "状态", width: "92px" },
  { key: "playtime", label: "游玩时长", sort: "playtime", align: "right", width: "112px" },
  { key: "rating", label: "评分", sort: "rating", align: "right", width: "84px" },
  { key: "lastPlayed", label: "最近游玩", sort: "lastPlayed", width: "112px" },
  { key: "release", label: "发售日期", sort: "release", width: "112px" },
  { key: "created", label: "入库时间", sort: "created", width: "112px" },
];

const allSelected = computed(
  () => props.games.length > 0 && props.games.every((g) => props.selectedIds.includes(g.id)),
);

function coverOf(game: Game): string | null {
  return game.coverPath ? convertFileSrc(game.coverPath) : null;
}

/** 无封面时的占位首字 */
function initialOf(game: Game): string {
  return game.title.trim().charAt(0) || "?";
}

function ariaSort(column: Column): "ascending" | "descending" | "none" | undefined {
  if (!column.sort) return undefined;
  if (props.sortBy !== column.sort) return "none";
  return props.sortDesc ? "descending" : "ascending";
}

function onHeaderClick(column: Column) {
  if (column.sort) emit("sort", column.sort);
}

function onRowClick(game: Game) {
  if (props.selectionMode) emit("select", game.id);
  else emit("open", game);
}
</script>

<template>
  <div class="overflow-hidden rounded-xl border border-line-soft">
    <table class="w-full border-collapse text-[12.5px]">
      <thead>
        <tr class="bg-surface-2/70 text-ink-3">
          <th v-if="selectionMode" scope="col" class="w-9 px-2.5 py-2">
            <input
              type="checkbox"
              class="h-3.5 w-3.5 cursor-pointer align-middle"
              :checked="allSelected"
              aria-label="全选当前列表"
              @change="emit('select', -1)"
            />
          </th>
          <th scope="col" class="w-14 px-2 py-2 text-left font-medium">封面</th>
          <th
            v-for="column in columns"
            :key="column.key"
            scope="col"
            class="px-3 py-2 font-medium whitespace-nowrap"
            :class="column.align === 'right' ? 'text-right' : 'text-left'"
            :style="column.width ? { width: column.width } : {}"
            :aria-sort="ariaSort(column)"
          >
            <button
              v-if="column.sort"
              class="inline-flex items-center gap-1 rounded transition hover:text-ink"
              :class="sortBy === column.sort ? 'text-accent' : ''"
              :aria-label="`按${column.label}排序`"
              @click="onHeaderClick(column)"
            >
              {{ column.label }}
              <svg
                v-if="sortBy === column.sort"
                width="9"
                height="9"
                viewBox="0 0 10 10"
                fill="none"
                aria-hidden="true"
                :style="{ transform: sortDesc ? 'scaleY(-1)' : 'none' }"
              >
                <path
                  d="M5 1.5v7M5 8.5L2.2 5.7M5 8.5l2.8-2.8"
                  stroke="currentColor"
                  stroke-width="1.4"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </button>
            <span v-else>{{ column.label }}</span>
          </th>
        </tr>
      </thead>

      <tbody>
        <tr
          v-for="(game, index) in games"
          :key="game.id"
          data-game-row
          :tabindex="focusedIndex === index ? 0 : -1"
          class="group cursor-pointer border-t border-line-soft/70 outline-none transition hover:bg-surface-2/60 focus-visible:bg-surface-2/70 focus-visible:ring-1 focus-visible:ring-accent/70 focus-visible:ring-inset"
          :class="selectedIds.includes(game.id) ? 'bg-accent/[0.07]' : ''"
          @click="onRowClick(game)"
          @focus="emit('focused', index)"
          @dblclick="emit('open', game)"
          @contextmenu.prevent="
            emit('contextmenu', { game, x: ($event as MouseEvent).clientX, y: ($event as MouseEvent).clientY })
          "
        >
          <td v-if="selectionMode" class="px-2.5 py-1.5" @click.stop>
            <input
              type="checkbox"
              class="h-3.5 w-3.5 cursor-pointer align-middle"
              :checked="selectedIds.includes(game.id)"
              :aria-label="`选择《${game.title}》`"
              @change="emit('select', game.id)"
            />
          </td>

          <!-- 封面缩略图 -->
          <td class="px-2 py-1.5">
            <div
              class="relative h-11 w-8 shrink-0 overflow-hidden rounded bg-gradient-to-br from-surface-3 to-surface-2"
            >
              <img
                v-if="coverOf(game)"
                :src="coverOf(game)!"
                :alt="`《${game.title}》封面`"
                class="h-full w-full object-cover"
                loading="lazy"
              />
              <span
                v-else
                class="absolute inset-0 flex items-center justify-center text-[13px] font-semibold text-ink-3"
                aria-hidden="true"
              >
                {{ initialOf(game) }}
              </span>
            </div>
          </td>

          <!-- 名称 + 标签 -->
          <td class="min-w-0 px-3 py-1.5">
            <div class="flex items-center gap-1.5">
              <span
                v-if="runningIds.has(game.id)"
                class="h-1.5 w-1.5 shrink-0 rounded-full bg-sage"
                title="正在运行"
                aria-label="正在运行"
              />
              <span class="truncate font-medium text-ink" :title="game.title">{{ game.title }}</span>
              <span
                v-if="game.favorite === 1"
                class="shrink-0 text-[11px] text-amber"
                aria-label="已收藏"
                title="已收藏"
                >★</span
              >
            </div>
            <div v-if="game.developer || game.tags.length" class="mt-0.5 flex items-center gap-1.5">
              <span v-if="game.developer" class="truncate text-[11px] text-ink-3">{{
                game.developer
              }}</span>
              <span
                v-for="tag in game.tags.slice(0, 3)"
                :key="tag.id"
                class="shrink-0 rounded px-1 text-[10.5px] text-ink-3"
                :style="{ background: (tag.color ?? '#8a7d6d') + '22' }"
                >{{ tag.name }}</span
              >
            </div>
          </td>

          <!-- 状态 -->
          <td class="px-3 py-1.5">
            <span class="inline-flex items-center gap-1.5 whitespace-nowrap">
              <span
                class="h-1.5 w-1.5 rounded-full"
                :style="{ background: STATUS_COLORS[game.playStatus] }"
                aria-hidden="true"
              />
              <span class="text-ink-2">{{ STATUS_LABELS[game.playStatus] }}</span>
            </span>
          </td>

          <td class="px-3 py-1.5 text-right tabular-nums text-ink-2">
            {{ formatDuration(game.totalPlaySeconds) }}
          </td>

          <td class="px-3 py-1.5 text-right tabular-nums text-ink-2">
            {{ game.rating > 0 ? game.rating : "—" }}
          </td>

          <td class="px-3 py-1.5 whitespace-nowrap text-ink-3">
            {{ formatRelative(game.lastPlayedAt) }}
          </td>

          <td class="px-3 py-1.5 whitespace-nowrap text-ink-3">
            {{ game.releaseDate ? formatDate(game.releaseDate) : "—" }}
          </td>

          <td class="px-3 py-1.5 whitespace-nowrap text-ink-3">
            {{ formatDate(game.createdAt) }}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
