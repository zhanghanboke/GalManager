<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import GameCard from "../components/GameCard.vue";
import GameTable from "../components/GameTable.vue";
import ScanDialog from "../components/ScanDialog.vue";
import GameEditDialog from "../components/GameEditDialog.vue";
import BatchBar from "../components/BatchBar.vue";
import ContextMenu from "../components/ContextMenu.vue";
import EmptyState from "../components/EmptyState.vue";
import { launchApi, type Game, type PlayStatus } from "../api";
import { useLibraryStore } from "../stores/library";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../composables/useConfirm";
import { pendingDropPaths } from "../composables/useDropImport";
import { errorText, toast } from "../utils/toast";
import { STATUS_COLORS, STATUS_LABELS, debounce } from "../utils/format";

const library = useLibraryStore();
const settings = useSettingsStore();
const router = useRouter();

const scanOpen = ref(false);
/** 由拖拽导入预填的根目录；关闭对话框时清空 */
const dropPaths = ref<string[]>([]);
const editing = ref<Game | null>(null);
const menu = ref<{ game: Game; x: number; y: number } | null>(null);

// 消费 App.vue 收到的拖入路径，直接带路径打开扫描对话框
watch(
  pendingDropPaths,
  (paths) => {
    if (!paths.length) return;
    dropPaths.value = [...paths];
    pendingDropPaths.value = [];
    scanOpen.value = true;
  },
  { immediate: true },
);

function closeScan() {
  scanOpen.value = false;
  dropPaths.value = [];
}

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

/** 列表视图点表头：同一列则切换升降序，否则换列 */
function onTableSort(value: string) {
  if (library.filter.sortBy === value) {
    void library.setFilter({ sortDesc: !library.filter.sortDesc });
  } else {
    changeSort(value);
  }
}

/** 列表视图的全选 / 单选（-1 表示表头全选） */
function onTableSelect(id: number) {
  if (id === -1) {
    const allSelected =
      library.games.length > 0 && library.games.every((g) => library.selectedIds.includes(g.id));
    if (allSelected) library.clearSelection();
    else library.selectAll();
    return;
  }
  library.toggleSelected(id);
}

function setViewMode(mode: "grid" | "list") {
  void settings.set("view_mode", mode);
}

function openDetail(game: Game) {
  router.push(`/game/${game.id}`);
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

/* ---------------- 键盘导航 ---------------- */

/** 当前聚焦的卡片 / 行下标（roving tabindex） */
const focusIndex = ref(0);
/** 列表滚动容器，方向键事件由它统一接收（卡片与表格行都会冒泡上来） */
const listRef = ref<HTMLElement | null>(null);
const searchField = ref<HTMLInputElement | null>(null);

function currentItems(): HTMLElement[] {
  const container = listRef.value;
  if (!container) return [];
  const selector = settings.viewMode === "list" ? "[data-game-row]" : "[data-game-card]";
  return Array.from(container.querySelectorAll<HTMLElement>(selector));
}

/** 封面墙当前列数，用于上下键按「行」跳 */
function columnCount(): number {
  const grid = listRef.value?.querySelector<HTMLElement>("[data-grid]");
  if (!grid) return 1;
  const columns = getComputedStyle(grid).gridTemplateColumns;
  const count = columns.split(/\s+/).filter((part) => part && part !== "none").length;
  return Math.max(1, count);
}

function focusItemAt(index: number) {
  const items = currentItems();
  if (!items.length) return;
  const clamped = Math.max(0, Math.min(index, items.length - 1));
  focusIndex.value = clamped;
  items[clamped]?.focus();
}

function isDialogOpen() {
  return scanOpen.value || editing.value !== null || menu.value !== null;
}

/** 方向键移动焦点；Enter / 空格激活当前项 */
function onKeydown(event: KeyboardEvent) {
  const items = currentItems();
  if (!items.length) return;

  if (event.key === "Enter" || event.key === " ") {
    const game = library.games[focusIndex.value];
    if (!game) return;
    event.preventDefault();
    if (event.key === "Enter") {
      // 多选模式下 Enter 用于勾选，避免误跳走
      if (library.selectionMode) library.toggleSelected(game.id);
      else openDetail(game);
    } else {
      // 空格切换选中；不在多选模式时先进入多选，让用户看到勾选结果
      if (!library.selectionMode) library.toggleSelectionMode();
      library.toggleSelected(game.id);
    }
    return;
  }

  // 列表视图只有一列，左右键不参与导航
  const singleColumn = settings.viewMode === "list";
  const step = singleColumn ? 1 : columnCount();
  const current = focusIndex.value;
  let next: number | null = null;

  switch (event.key) {
    case "ArrowRight":
      if (singleColumn) return;
      next = current + 1;
      break;
    case "ArrowLeft":
      if (singleColumn) return;
      next = current - 1;
      break;
    case "ArrowDown":
      next = current + step;
      break;
    case "ArrowUp":
      next = current - step;
      break;
    case "Home":
      next = 0;
      break;
    case "End":
      next = items.length - 1;
      break;
    default:
      return;
  }

  event.preventDefault();
  focusItemAt(next);
}

/** 全局快捷键：Ctrl/Cmd+F 或 / 聚焦搜索，Esc 收尾 */
function onGlobalKeydown(event: KeyboardEvent) {
  const target = event.target as HTMLElement | null;
  const typing =
    !!target &&
    (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable);

  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
    event.preventDefault();
    searchField.value?.focus();
    searchField.value?.select();
    return;
  }

  if (event.key === "/" && !typing && !isDialogOpen()) {
    event.preventDefault();
    searchField.value?.focus();
    return;
  }

  if (event.key === "Escape") {
    // 对话框自己处理 Esc（BaseModal 已监听），这里不要抢
    if (isDialogOpen()) return;
    if (typing) {
      target?.blur();
      return;
    }
    if (library.selectionMode) {
      library.toggleSelectionMode();
      return;
    }
    // 退出卡片焦点，让 Tab 从工具栏重新开始
    if (target?.closest("[data-game-card],[data-game-row]")) target.blur();
  }
}

// 筛选 / 搜索导致列表变短后，把焦点下标夹回合法范围
watch(
  () => library.games.length,
  (length) => {
    if (length && focusIndex.value >= length) focusIndex.value = 0;
  },
);

onMounted(() => {
  window.addEventListener("click", closeMenu);
  window.addEventListener("scroll", closeMenu, true);
  window.addEventListener("keydown", onGlobalKeydown);
});
onUnmounted(() => {
  window.removeEventListener("click", closeMenu);
  window.removeEventListener("scroll", closeMenu, true);
  window.removeEventListener("keydown", onGlobalKeydown);
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
            ref="searchField"
            v-model="searchInput"
            class="field pl-8"
            type="search"
            aria-label="搜索游戏"
            placeholder="搜索游戏名、拼音首字母或开发商…（Ctrl+F）"
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
          :aria-label="library.filter.sortDesc ? '切换为升序排列' : '切换为降序排列'"
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

        <!-- 视图切换：封面墙 / 列表 -->
        <div
          class="flex items-center gap-0.5 rounded-lg border border-line-soft bg-surface-2/60 p-0.5"
          role="group"
          aria-label="切换呈现方式"
        >
          <button
            class="rounded-md p-1.5 transition"
            :class="settings.viewMode === 'grid' ? 'bg-surface-3 text-accent' : 'text-ink-3 hover:text-ink'"
            :aria-pressed="settings.viewMode === 'grid'"
            aria-label="封面墙视图"
            title="封面墙视图"
            @click="setViewMode('grid')"
          >
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <rect x="1" y="1" width="5" height="5" rx="1.2" stroke="currentColor" stroke-width="1.4" />
              <rect x="8" y="1" width="5" height="5" rx="1.2" stroke="currentColor" stroke-width="1.4" />
              <rect x="1" y="8" width="5" height="5" rx="1.2" stroke="currentColor" stroke-width="1.4" />
              <rect x="8" y="8" width="5" height="5" rx="1.2" stroke="currentColor" stroke-width="1.4" />
            </svg>
          </button>
          <button
            class="rounded-md p-1.5 transition"
            :class="settings.viewMode === 'list' ? 'bg-surface-3 text-accent' : 'text-ink-3 hover:text-ink'"
            :aria-pressed="settings.viewMode === 'list'"
            aria-label="列表视图"
            title="列表视图"
            @click="setViewMode('list')"
          >
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <path
                d="M1.5 3h11M1.5 7h11M1.5 11h11"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
              />
            </svg>
          </button>
        </div>

        <div class="ml-auto flex items-center gap-2">
          <button
            class="btn btn-ghost"
            :class="library.selectionMode ? 'border-accent/50 text-accent' : ''"
            :aria-pressed="library.selectionMode"
            aria-label="多选模式"
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
          class="ml-1 text-[13px] text-accent hover:underline"
          @click="
            () => {
              searchInput = '';
              library.resetFilter();
            }
          "
        >
          清除筛选
        </button>

        <span class="ml-auto text-[13px] text-ink-3">
          共 {{ library.games.length }} 个游戏
        </span>
      </div>
    </header>

    <!-- 封面墙 / 列表 -->
    <div ref="listRef" class="min-h-0 flex-1 scroll-y px-5 py-4" @keydown="onKeydown">
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

      <div
        v-else-if="settings.viewMode === 'grid'"
        data-grid
        class="grid gap-x-3.5 gap-y-5"
        :style="gridStyle"
      >
        <GameCard
          v-for="(game, index) in library.games"
          :key="game.id"
          :game="game"
          :index="index"
          :focused="index === focusIndex"
          :running="settings.runningIds.has(game.id)"
          :selection-mode="library.selectionMode"
          :selected="library.selectedIds.includes(game.id)"
          @open="openDetail"
          @launch="handleLaunch"
          @select="library.toggleSelected(game.id)"
          @focused="focusIndex = $event"
          @contextmenu="menu = $event"
        />
      </div>

      <GameTable
        v-else
        :games="library.games"
        :running-ids="settings.runningIds"
        :selection-mode="library.selectionMode"
        :selected-ids="library.selectedIds"
        :sort-by="library.filter.sortBy ?? 'title'"
        :sort-desc="library.filter.sortDesc ?? false"
        :focused-index="focusIndex"
        @open="openDetail"
        @launch="handleLaunch"
        @select="onTableSelect"
        @focused="focusIndex = $event"
        @contextmenu="menu = $event"
        @sort="onTableSort"
      />

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

    <ScanDialog
      v-if="scanOpen"
      :initial-paths="dropPaths"
      @close="closeScan"
      @done="library.refresh()"
    />

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
