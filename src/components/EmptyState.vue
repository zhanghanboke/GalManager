<script setup lang="ts">
defineProps<{ hasFilter?: boolean }>();
const emit = defineEmits<{ add: []; clear: [] }>();
</script>

<template>
  <div class="anim-fade flex flex-col items-center justify-center py-20 text-center">
    <div
      class="mb-5 flex h-16 w-16 items-center justify-center rounded-2xl border border-line bg-surface-2"
    >
      <svg width="28" height="28" viewBox="0 0 28 28" fill="none" class="text-ink-3">
        <rect x="3" y="4" width="22" height="20" rx="3" stroke="currentColor" stroke-width="1.6" />
        <path d="M3 9.5h22" stroke="currentColor" stroke-width="1.6" />
        <circle cx="6.6" cy="6.8" r="1" fill="currentColor" />
        <circle cx="10" cy="6.8" r="1" fill="currentColor" />
        <path d="M9 20l3.4-4 3 3.4 2.2-2.4 2.4 3" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </div>

    <template v-if="hasFilter">
      <h3 class="text-[14px] font-medium text-ink">没有匹配的游戏</h3>
      <p class="mt-1.5 max-w-[320px] text-[12.5px] leading-relaxed text-ink-3">
        试试换个关键词，或者清除当前的筛选条件。
      </p>
      <button class="btn btn-ghost mt-4" @click="emit('clear')">清除筛选</button>
    </template>

    <template v-else>
      <h3 class="text-[14px] font-medium text-ink">游戏库还是空的</h3>
      <p class="mt-1.5 max-w-[340px] text-[12.5px] leading-relaxed text-ink-3">
        扫描本地文件夹，GalManager 会自动识别游戏目录、启动程序与引擎类型。
      </p>
      <button class="btn btn-primary mt-4" @click="emit('add')">
        <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
          <path d="M7 2.5v9M2.5 7h9" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
        </svg>
        扫描添加游戏
      </button>
    </template>
  </div>
</template>
