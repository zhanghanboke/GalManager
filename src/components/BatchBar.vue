<script setup lang="ts">
import { ref } from "vue";
import { useLibraryStore } from "../stores/library";
import { errorText, toast } from "../utils/toast";
import type { PlayStatus } from "../api";
import { STATUS_LABELS } from "../utils/format";

const emit = defineEmits<{ edit: [] }>();

const library = useLibraryStore();
const tagInput = ref("");
const showTagInput = ref(false);

const statuses: PlayStatus[] = ["unplayed", "playing", "completed", "on_hold", "dropped"];

async function run(action: () => Promise<void>) {
  try {
    await action();
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function applyTags() {
  const names = tagInput.value
    .split(/[,，\s]+/)
    .map((s) => s.trim())
    .filter(Boolean);
  if (!names.length) return;
  await run(() => library.batchTags(names));
  tagInput.value = "";
  showTagInput.value = false;
}
</script>

<template>
  <Transition name="bar">
    <footer
      v-if="library.selectionMode"
      class="flex shrink-0 items-center gap-2 border-t border-line bg-surface px-5 py-2.5"
    >
      <span class="text-[12.5px] text-ink-2">
        已选 <span class="font-semibold text-accent">{{ library.selectedIds.length }}</span> 个
      </span>

      <button class="btn btn-ghost h-7 px-2.5 text-[12px]" @click="library.selectAll()">
        全选
      </button>
      <button
        class="btn btn-ghost h-7 px-2.5 text-[12px]"
        :disabled="!library.selectedIds.length"
        @click="library.clearSelection()"
      >
        清空
      </button>

      <span class="mx-1 h-4 w-px bg-line" />

      <!-- 分类 -->
      <select
        class="field h-7 w-[110px] cursor-pointer text-[12px]"
        :disabled="!library.selectedIds.length"
        @change="
          (e) => {
            const v = (e.target as HTMLSelectElement).value;
            if (!v) return;
            run(() => library.batchCategory(v === '__none__' ? null : Number(v)));
          }
        "
      >
        <option value="" disabled>设为分类…</option>
        <option :value="'__none__'">未分类</option>
        <option v-for="c in library.categories" :key="c.id" :value="c.id">{{ c.name }}</option>
      </select>

      <!-- 状态 -->
      <select
        class="field h-7 w-[104px] cursor-pointer text-[12px]"
        :disabled="!library.selectedIds.length"
        @change="
          (e) => {
            const v = (e.target as HTMLSelectElement).value as PlayStatus;
            if (v) run(() => library.batchStatus(v));
          }
        "
      >
        <option value="">设为状态…</option>
        <option v-for="s in statuses" :key="s" :value="s">{{ STATUS_LABELS[s] }}</option>
      </select>

      <button
        class="btn btn-ghost h-7 px-2.5 text-[12px]"
        :disabled="!library.selectedIds.length"
        @click="run(() => library.batchFavorite(true))"
      >
        收藏
      </button>

      <template v-if="showTagInput">
        <input
          v-model="tagInput"
          class="field h-7 w-[150px] text-[12px]"
          placeholder="标签，逗号分隔"
          autofocus
          @keydown.enter="applyTags"
          @blur="showTagInput = false"
        />
      </template>
      <button
        v-else
        class="btn btn-ghost h-7 px-2.5 text-[12px]"
        :disabled="!library.selectedIds.length"
        @click="showTagInput = true"
      >
        加标签
      </button>

      <button
        class="btn btn-ghost h-7 px-2.5 text-[12px]"
        :disabled="library.selectedIds.length !== 1"
        @click="emit('edit')"
      >
        编辑
      </button>

      <button class="btn btn-ghost ml-auto h-7 px-2.5 text-[12px]" @click="library.toggleSelectionMode(false)">
        退出多选
      </button>
    </footer>
  </Transition>
</template>

<style scoped>
.bar-enter-active,
.bar-leave-active {
  transition: all 0.2s cubic-bezier(0.22, 1, 0.36, 1);
}
.bar-enter-from,
.bar-leave-to {
  opacity: 0;
  transform: translateY(10px);
}
</style>
