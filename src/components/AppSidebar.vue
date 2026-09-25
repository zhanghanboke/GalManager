<script setup lang="ts">
import { computed, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useLibraryStore } from "../stores/library";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../composables/useConfirm";
import { errorText, toast } from "../utils/toast";
import { colorOf } from "../utils/format";

const library = useLibraryStore();
const settings = useSettingsStore();
const router = useRouter();
const route = useRoute();

const navItems = [
  { name: "library", label: "游戏库", to: "/", icon: "grid" },
  { name: "stats", label: "数据统计", to: "/stats", icon: "chart" },
  { name: "settings", label: "设置", to: "/settings", icon: "gear" },
  { name: "about", label: "关于", to: "/about", icon: "info" },
] as const;

const addingCategory = ref(false);
const newCategoryName = ref("");

const activeCategory = computed(() => library.filter.categoryId ?? null);

function selectCategory(id: number | null) {
  void library.setFilter({ categoryId: id });
  if (route.name !== "library") void router.push("/");
}

function toggleTag(name: string) {
  const current = library.filter.tags ?? [];
  const next = current.includes(name)
    ? current.filter((t) => t !== name)
    : [...current, name];
  void library.setFilter({ tags: next });
  if (route.name !== "library") void router.push("/");
}

async function submitCategory() {
  const name = newCategoryName.value.trim();
  if (!name) return;
  try {
    await library.addCategory(name);
    newCategoryName.value = "";
    addingCategory.value = false;
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function removeCategory(id: number, name: string) {
  const ok = await confirmDialog({
    title: "删除分类",
    message: `确定删除分类「${name}」吗？\n该分类下的游戏不会被删除，会归入「未分类」。`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;
  try {
    await library.removeCategory(id);
    if (activeCategory.value === id) selectCategory(null);
  } catch (error) {
    toast.error(errorText(error));
  }
}
</script>

<template>
  <aside class="flex w-[228px] shrink-0 flex-col border-r border-line-soft bg-surface">
    <nav class="px-2.5 pt-3">
      <RouterLink
        v-for="item in navItems"
        :key="item.name"
        :to="item.to"
        class="group relative mb-0.5 flex h-9 items-center gap-2.5 rounded-[9px] px-2.5 text-[13px] transition"
        :class="
          route.name === item.name
            ? 'bg-surface-3 font-medium text-ink'
            : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
        "
      >
        <!-- 选中态左侧指示条 -->
        <span
          class="absolute top-1/2 -left-2.5 h-4 w-[2.5px] -translate-y-1/2 rounded-r-full bg-gradient-to-b from-accent to-clay transition-opacity"
          :class="route.name === item.name ? 'opacity-100' : 'opacity-0'"
        />
        <svg
          width="15"
          height="15"
          viewBox="0 0 16 16"
          fill="none"
          class="shrink-0"
          :class="route.name === item.name ? 'text-accent' : 'text-ink-3'"
        >
          <template v-if="item.icon === 'grid'">
            <rect x="1.5" y="1.5" width="5.5" height="5.5" rx="1.4" fill="currentColor" />
            <rect x="9" y="1.5" width="5.5" height="5.5" rx="1.4" fill="currentColor" opacity=".55" />
            <rect x="1.5" y="9" width="5.5" height="5.5" rx="1.4" fill="currentColor" opacity=".55" />
            <rect x="9" y="9" width="5.5" height="5.5" rx="1.4" fill="currentColor" />
          </template>
          <template v-else-if="item.icon === 'chart'">
            <rect x="1.5" y="8" width="3" height="6.5" rx="1.1" fill="currentColor" opacity=".55" />
            <rect x="6.5" y="4" width="3" height="10.5" rx="1.1" fill="currentColor" />
            <rect x="11.5" y="1.5" width="3" height="13" rx="1.1" fill="currentColor" opacity=".55" />
          </template>
          <template v-else-if="item.icon === 'gear'">
            <circle cx="8" cy="8" r="2.4" fill="currentColor" />
            <path
              d="M8 1.6v1.9M8 12.5v1.9M14.4 8h-1.9M3.5 8H1.6M12.5 3.5l-1.3 1.3M4.8 11.2l-1.3 1.3M12.5 12.5l-1.3-1.3M4.8 4.8L3.5 3.5"
              stroke="currentColor"
              stroke-width="1.4"
              stroke-linecap="round"
            />
          </template>
          <template v-else>
            <circle cx="8" cy="8" r="6.4" stroke="currentColor" stroke-width="1.4" />
            <path
              d="M8 7.2v4M8 5.1v.01"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
            />
          </template>
        </svg>
        <span class="flex-1">{{ item.label }}</span>
        <span v-if="item.name === 'library'" class="text-[11px] text-ink-3">
          {{ library.counts.total }}
        </span>
      </RouterLink>
    </nav>

    <div class="mx-4 my-3 h-px bg-line-soft" />

    <!-- 分类 -->
    <div class="flex min-h-0 flex-1 flex-col">
      <div class="flex items-center justify-between px-4 pb-1.5">
        <span class="text-[10.5px] font-semibold tracking-widest text-ink-3 uppercase">分类</span>
        <button
          class="flex h-5 w-5 items-center justify-center rounded-md text-ink-3 transition hover:bg-surface-3 hover:text-ink"
          title="新建分类"
          @click="addingCategory = !addingCategory"
        >
          <svg width="11" height="11" viewBox="0 0 12 12" fill="none">
            <path d="M6 1v10M1 6h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
          </svg>
        </button>
      </div>

      <form v-if="addingCategory" class="px-3 pb-1.5" @submit.prevent="submitCategory">
        <input
          v-model="newCategoryName"
          class="field h-7 text-[12px]"
          placeholder="分类名，回车确认"
          autofocus
          @blur="addingCategory = false"
        />
      </form>

      <div class="min-h-0 flex-1 scroll-y px-2.5">
        <button
          class="mb-0.5 flex h-8 w-full items-center gap-2 rounded-lg px-2.5 text-[12.5px] transition"
          :class="
            activeCategory === null
              ? 'bg-surface-3 text-ink'
              : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
          "
          @click="selectCategory(null)"
        >
          <span class="h-1.5 w-1.5 rounded-full bg-ink-3" />
          <span class="flex-1 text-left">全部游戏</span>
          <span class="text-[11px] text-ink-3">{{ library.counts.total }}</span>
        </button>

        <button
          class="mb-0.5 flex h-8 w-full items-center gap-2 rounded-lg px-2.5 text-[12.5px] transition"
          :class="
            activeCategory === -1
              ? 'bg-surface-3 text-ink'
              : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
          "
          @click="selectCategory(-1)"
        >
          <span class="h-1.5 w-1.5 rounded-full bg-ink-3/60" />
          <span class="flex-1 text-left">未分类</span>
          <span class="text-[11px] text-ink-3">{{ library.counts.uncategorized }}</span>
        </button>

        <div
          v-for="category in library.categories"
          :key="category.id"
          class="group mb-0.5 flex h-8 items-center rounded-lg transition"
          :class="
            activeCategory === category.id
              ? 'bg-surface-3 text-ink'
              : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
          "
        >
          <button
            class="flex h-full min-w-0 flex-1 items-center gap-2 px-2.5 text-[12.5px]"
            @click="selectCategory(category.id)"
          >
            <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-accent" />
            <span class="flex-1 truncate text-left">{{ category.name }}</span>
            <span class="text-[11px] text-ink-3">{{ category.gameCount }}</span>
          </button>
          <button
            class="mr-1.5 hidden h-5 w-5 shrink-0 items-center justify-center rounded-md text-ink-3 transition group-hover:flex hover:bg-base hover:text-danger"
            title="删除分类"
            @click.stop="removeCategory(category.id, category.name)"
          >
            <svg width="10" height="10" viewBox="0 0 12 12" fill="none">
              <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
            </svg>
          </button>
        </div>

        <p v-if="!library.categories.length" class="px-2.5 py-1.5 text-[11.5px] text-ink-3">
          还没有分类，点上方 + 新建
        </p>

        <!-- 标签 -->
        <template v-if="library.tags.length">
          <div class="mt-4 flex items-center justify-between px-2.5 pb-1.5">
            <span class="text-[10.5px] font-semibold tracking-widest text-ink-3 uppercase">
              标签
            </span>
            <button
              v-if="(library.filter.tags ?? []).length"
              class="text-[10.5px] text-accent hover:underline"
              @click="library.setFilter({ tags: [] })"
            >
              清除
            </button>
          </div>
          <div class="flex flex-wrap gap-1.5 px-2.5 pb-4">
            <button
              v-for="tag in library.tags.slice(0, 40)"
              :key="tag.id"
              class="chip transition hover:brightness-125"
              :style="
                (library.filter.tags ?? []).includes(tag.name)
                  ? {
                      background: colorOf(tag.name) + '2e',
                      color: colorOf(tag.name),
                      boxShadow: `inset 0 0 0 1px ${colorOf(tag.name)}55`,
                    }
                  : {}
              "
              @click="toggleTag(tag.name)"
            >
              {{ tag.name }}
              <span class="opacity-50">{{ tag.gameCount }}</span>
            </button>
          </div>
        </template>
      </div>
    </div>

    <!-- 运行中 -->
    <div v-if="settings.running.length" class="border-t border-line-soft px-3 py-3">
      <div class="mb-2 flex items-center gap-1.5">
        <span class="relative flex h-1.5 w-1.5">
          <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-sage opacity-70" />
          <span class="relative inline-flex h-1.5 w-1.5 rounded-full bg-sage" />
        </span>
        <span class="text-[10.5px] font-semibold tracking-widest text-ink-3 uppercase">
          运行中
        </span>
      </div>
      <RouterLink
        v-for="item in settings.running"
        :key="item.gameId"
        :to="`/game/${item.gameId}`"
        class="flex items-center gap-2 rounded-lg px-2 py-1.5 transition hover:bg-surface-2"
      >
        <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-sage" />
        <span class="min-w-0 flex-1 truncate text-[12px] text-ink-2">{{ item.title }}</span>
      </RouterLink>
    </div>
  </aside>
</template>
