/** 游戏库状态：列表、分类、标签、筛选条件、多选 */

import { defineStore } from "pinia";
import { computed, ref } from "vue";
import {
  categoryApi,
  libraryApi,
  tagApi,
  type Category,
  type Game,
  type GameFilter,
  type PlayStatus,
  type Tag,
} from "../api";
import { errorText, toast } from "../utils/toast";

export const useLibraryStore = defineStore("library", () => {
  const games = ref<Game[]>([]);
  const categories = ref<Category[]>([]);
  const tags = ref<Tag[]>([]);
  const loading = ref(false);
  const loaded = ref(false);

  /** 当前筛选条件 */
  const filter = ref<GameFilter>({
    keyword: "",
    categoryId: null,
    tags: [],
    statuses: [],
    favoriteOnly: false,
    engines: [],
    sortBy: "title",
    sortDesc: false,
  });

  /** 多选模式 */
  const selectionMode = ref(false);
  const selectedIds = ref<number[]>([]);

  const selectedGames = computed(() =>
    games.value.filter((game) => selectedIds.value.includes(game.id)),
  );

  /** 侧边栏用的统计 */
  const counts = computed(() => {
    const all = games.value;
    return {
      total: all.length,
      favorite: all.filter((g) => g.favorite === 1).length,
      playing: all.filter((g) => g.playStatus === "playing").length,
      completed: all.filter((g) => g.playStatus === "completed").length,
      unplayed: all.filter((g) => g.playStatus === "unplayed").length,
      uncategorized: all.filter((g) => g.categoryId === null).length,
    };
  });

  async function refresh() {
    loading.value = true;
    try {
      const [nextGames, nextCategories, nextTags] = await Promise.all([
        libraryApi.list(filter.value),
        categoryApi.list(),
        tagApi.list(),
      ]);
      games.value = nextGames;
      categories.value = nextCategories;
      tags.value = nextTags;
      loaded.value = true;
    } catch (error) {
      toast.error(`加载游戏库失败：${errorText(error)}`);
    } finally {
      loading.value = false;
    }
  }

  /** 只刷新游戏列表（筛选变化时用，避免重复拉分类） */
  async function refreshGames() {
    loading.value = true;
    try {
      games.value = await libraryApi.list(filter.value);
    } catch (error) {
      toast.error(`加载游戏列表失败：${errorText(error)}`);
    } finally {
      loading.value = false;
    }
  }

  function setFilter(patch: Partial<GameFilter>) {
    filter.value = { ...filter.value, ...patch };
    return refreshGames();
  }

  function resetFilter() {
    filter.value = {
      keyword: "",
      categoryId: null,
      tags: [],
      statuses: [],
      favoriteOnly: false,
      engines: [],
      sortBy: "title",
      sortDesc: false,
    };
    return refreshGames();
  }

  /** 是否处于「非默认」筛选态，用于显示「清除筛选」 */
  const hasActiveFilter = computed(() => {
    const f = filter.value;
    return Boolean(
      f.keyword ||
        f.categoryId !== null ||
        f.favoriteOnly ||
        (f.tags && f.tags.length) ||
        (f.statuses && f.statuses.length) ||
        (f.engines && f.engines.length),
    );
  });

  // ---------- 多选 ----------

  function toggleSelectionMode(force?: boolean) {
    selectionMode.value = force ?? !selectionMode.value;
    if (!selectionMode.value) selectedIds.value = [];
  }

  function toggleSelected(id: number) {
    const index = selectedIds.value.indexOf(id);
    if (index >= 0) selectedIds.value.splice(index, 1);
    else selectedIds.value.push(id);
  }

  function clearSelection() {
    selectedIds.value = [];
  }

  function selectAll() {
    selectedIds.value = games.value.map((game) => game.id);
  }

  // ---------- 变更操作 ----------

  async function removeGame(id: number) {
    await libraryApi.remove(id);
    toast.success("已从库中移除");
    await refresh();
  }

  async function batchCategory(categoryId: number | null) {
    await libraryApi.batchCategory(selectedIds.value, categoryId);
    toast.success(`已更新 ${selectedIds.value.length} 个游戏的分类`);
    clearSelection();
    await refresh();
  }

  async function batchStatus(status: PlayStatus) {
    await libraryApi.batchStatus(selectedIds.value, status);
    toast.success(`已更新 ${selectedIds.value.length} 个游戏的状态`);
    clearSelection();
    await refresh();
  }

  async function batchFavorite(favorite: boolean) {
    await libraryApi.batchFavorite(selectedIds.value, favorite);
    toast.success(favorite ? "已加入收藏" : "已取消收藏");
    clearSelection();
    await refresh();
  }

  async function batchTags(names: string[]) {
    await libraryApi.batchTags(selectedIds.value, names);
    toast.success(`已为 ${selectedIds.value.length} 个游戏添加标签`);
    clearSelection();
    await refresh();
  }

  async function addCategory(name: string) {
    await categoryApi.create(name);
    categories.value = await categoryApi.list();
    toast.success(`已创建分类「${name}」`);
  }

  async function renameCategory(id: number, name: string) {
    await categoryApi.update(id, name);
    categories.value = await categoryApi.list();
  }

  async function removeCategory(id: number) {
    await categoryApi.remove(id);
    categories.value = await categoryApi.list();
    await refreshGames();
    toast.success("分类已删除，原有游戏已归入「未分类」");
  }

  return {
    games,
    categories,
    tags,
    loading,
    loaded,
    filter,
    selectionMode,
    selectedIds,
    selectedGames,
    counts,
    hasActiveFilter,
    refresh,
    refreshGames,
    setFilter,
    resetFilter,
    toggleSelectionMode,
    toggleSelected,
    clearSelection,
    selectAll,
    removeGame,
    batchCategory,
    batchStatus,
    batchFavorite,
    batchTags,
    addCategory,
    renameCategory,
    removeCategory,
  };
});
