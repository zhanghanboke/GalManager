<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import GameCard from "../components/GameCard.vue";
import ScanDialog from "../components/ScanDialog.vue";
import GameEditDialog from "../components/GameEditDialog.vue";
import BatchBar from "../components/BatchBar.vue";
import ContextMenu from "../components/ContextMenu.vue";
import EmptyState from "../components/EmptyState.vue";
import { launchApi, type Game, type PlayStatus } from "../api";
import { useLibraryStore } from "../stores/library";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../composables/useConfirm";
import { errorText, toast } from "../utils/toast";
import { STATUS_COLORS, STATUS_LABELS, debounce } from "../utils/format";

const library = useLibraryStore();
const settings = useSettingsStore();
const router = useRouter();

const scanOpen = ref(false);
const editing = ref<Game | null>(null);
const menu = ref<{ game: Game; x: number; y: number } | null>(null);

const sortOptions = [
  { value: "title", label: "名称" },
  { value: "lastPlayed", label: "最近游玩" },
  { value: "playtime", label: "游玩时长" },
  { value: "rating", label: "评分" },
  { value: "release", label: "发售日期" },
  { value: "created", label: "入库时间" },
];

const statusOptions: PlayStatus[] = ["unplayed", "playing", "completed", "on_hold", "dropped"];

/** 网格列宽随设置变化 */
const gridStyle = computed(() => {
  const map: Record<string, string> = {
    sm: "repeat(auto-fill, minmax(112px, 1fr))",
    md: "repeat(auto-fill, minmax(148px, 1fr))",
    lg: "repeat(auto-fill, minmax(196px, 1fr))",
  };
  return { gridTemplateColumns: map[settings.gridSize] ?? map.md };
});

const searchInput = ref(library.filter.keyword ?? "");
const applySearch = debounce(() => {
  void library.setFilter({ keyword: searchInput.value });
}, 260);

function toggleStatus(status: PlayStatus) {
  const current = library.filter.statuses ?? [];
  const next = current.includes(status)
    ? current.filter((s) => s !== status)
    : [...current, status];
  void library.setFilter({ statuses: next });
}

function changeSort(value: string) {
  void library.setFilter({ sortBy: value, sortDesc: value === "created" });
}

async function handleLaunch(game: Game) {
  if (settings.runningIds.has(game.id)) {
    try {
      await launchApi.stop(game.id);
      await settings.refreshRunning();
      toast.info(`已停止《${game.title}》`);
    } catch (error) {
      toast.error(errorText(error));
    }
    return;
  }
  try {
    const outcome = await launchApi.launch(game.id);
    if (outcome.ok) {
      toast.success(outcome.message);
      await settings.refreshRunning();
      void library.refreshGames();
    } else {
      toast.warn(outcome.message);
    }
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function handleDelete(game: Game) {
  const ok = await confirmDialog({
    title: "从库中移除",
    message: `确定将《${game.title}》从游戏库中移除吗？\n\n仅移除记录，不会删除磁盘上的游戏文件。`,
    confirmText: "移除",
    danger: true,
  });
  if (!ok) return;
  try {
    await library.removeGame(game.id);
  } catch (error) {
    toast.error(errorText(error));
  }
}

function closeMenu() {
  menu.value = null;
}

onMounted(() => {
  window.addEventListener("click", closeMenu);
  window.addEventListener("scroll", closeMenu, true);
});
onUnmounted(() => {
  window.removeEventListener("click", closeMenu);
  window.removeEventListener("scroll", closeMenu, true);
});
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col">
    <!-- 工具栏 -->
    <header class="shrink-0 border-b border-line-soft bg-surface/35 px-5 pt-3.5 pb-3">
      <div class="flex items-center gap-2.5">
        <div class="relative flex-1 max-w-[380px]">
          <svg
            class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-ink-3"
            width="13"
            height="13"
            viewBox="0 0 14 14"
            fill="none"
          >
            <circle cx="6" cy="6" r="4.6" stroke="currentColor" stroke-width="1.5" />
            <path d="M9.5 9.5L12.6 12.6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
          <input
            v-model="searchInput"
            class="field pl-8"
            placeholder="搜索游戏名、开发商或路径…"
            @input="applySearch"
          />
        </div>

        <select
          class="field w-[122px] cursor-pointer"
          :value="library.filter.sortBy"
          @change="changeSort(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="option in sortOptions" :key="option.value" :value="option.value">
            {{ option.label }}
          </option>
        </select>

        <button
          class="btn btn-ghost px-2.5"
          :title="library.filter.sortDesc ? '当前：降序' : '当前：升序'"
          @click="library.setFilter({ sortDesc: !library.filter.sortDesc })"
        >
          <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
            <path
              d="M7 2v10M7 12l-3.2-3.2M7 12l3.2-3.2"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              :style="{ transform: library.filter.sortDesc ? 'scaleY(-1)' : 'none', transformOrigin: 'center' }"
            />
          </svg>
        </button>

        <div class="ml-auto flex items-center gap-2">
          <button
            class="btn btn-ghost"
            :class="library.selectionMode ? 'border-accent/50 text-accent' : ''"
            title="多选模式"
            @click="library.toggleSelectionMode()"
          >
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
              <rect x="1.5" y="1.5" width="11" height="11" rx="2.6" stroke="currentColor" stroke-width="1.5" />
              <path d="M4.4 7.2l1.9 1.9 3.5-4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            多选
          </button>

          <button class="btn btn-primary" @click="scanOpen = true">
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
              <path d="M7 2.5v9M2.5 7h9" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
            </svg>
            添加游戏
          </button>
        </div>
      </div>

      <!-- 状态筛选 -->
      <div class="mt-2.5 flex flex-wrap items-center gap-1.5">
        <button
          class="chip transition"
          :class="library.filter.favoriteOnly ? 'bg-amber/20 text-amber' : 'hover:bg-surface-3'"
          @click="library.setFilter({ favoriteOnly: !library.filter.favoriteOnly })"
        >
          <svg width="10" height="10" viewBox="0 0 12 12" :fill="library.filter.favoriteOnly ? '#e8b04b' : 'currentColor'">
            <path d="M6 .8l1.6 3.3 3.6.5-2.6 2.5.6 3.6L6 9l-3.2 1.7.6-3.6L.8 4.6l3.6-.5z" />
          </svg>
          收藏
        </button>

        <span class="mx-0.5 h-3.5 w-px bg-line" />

        <button
          v-for="status in statusOptions"
          :key="status"
          class="chip transition"
          :style="
            (library.filter.statuses ?? []).includes(status)
              ? {
                  background: STATUS_COLORS[status] + '24',
                  color: STATUS_COLORS[status],
                }
              : {}
          "
          @click="toggleStatus(status)"
        >
          <span class="h-1.5 w-1.5 rounded-full" :style="{ background: STATUS_COLORS[status] }" />
          {{ STATUS_LABELS[status] }}
        </button>

        <button
          v-if="library.hasActiveFilter"
          class="ml-1 text-[11.5px] text-accent hover:underline"
          @click="
            () => {
              searchInput = '';
              library.resetFilter();
            }
          "
        >
          清除筛选
        </button>

        <span class="ml-auto text-[11.5px] text-ink-3">
          共 {{ library.games.length }} 个游戏
        </span>
      </div>
    </header>

    <!-- 封面墙 -->
    <div class="min-h-0 flex-1 scroll-y px-5 py-4">
      <EmptyState
        v-if="!library.games.length && !library.loading"
        :has-filter="library.hasActiveFilter"
        @add="scanOpen = true"
        @clear="
          () => {
            searchInput = '';
            library.resetFilter();
          }
        "
      />

      <div v-else class="grid gap-x-3.5 gap-y-5" :style="gridStyle">
        <GameCard
          v-for="(game, index) in library.games"
          :key="game.id"
          :game="game"
          :index="index"
          :running="settings.runningIds.has(game.id)"
          :selection-mode="library.selectionMode"
          :selected="library.selectedIds.includes(game.id)"
          @open="router.push(`/game/${game.id}`)"
          @launch="handleLaunch"
          @select="library.toggleSelected(game.id)"
          @contextmenu="menu = $event"
        />
      </div>

      <div v-if="library.loading" class="flex justify-center py-8">
        <span class="anim-spin h-5 w-5 rounded-full border-2 border-line border-t-accent" />
      </div>
    </div>

    <BatchBar @edit="editing = library.selectedGames[0] ?? null" />

    <ContextMenu
      v-if="menu"
      :game="menu.game"
      :x="menu.x"
      :y="menu.y"
      :running="settings.runningIds.has(menu.game.id)"
      @close="closeMenu"
      @open-detail="router.push(`/game/${menu!.game.id}`)"
      @launch="handleLaunch(menu!.game)"
      @edit="editing = menu!.game"
      @remove="handleDelete(menu!.game)"
      @open-folder="launchApi.openGameFolder(menu!.game.id).catch((e) => toast.error(errorText(e)))"
    />

    <ScanDialog v-if="scanOpen" @close="scanOpen = false" @done="library.refresh()" />

    <GameEditDialog
      v-if="editing"
      :game="editing"
      @close="editing = null"
      @saved="
        () => {
          editing = null;
          library.refresh();
        }
      "
    />
  </div>
</template>
