<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { statsApi, type PlaySession } from "../../api";
import { confirmDialog } from "../../composables/useConfirm";
import { errorText, toast } from "../../utils/toast";
import { formatDateTime, formatDuration } from "../../utils/format";

const props = defineProps<{ gameId: number }>();

const sessions = ref<PlaySession[]>([]);
const loading = ref(false);

/** 按日期分组，形成「时间线」 */
const grouped = computed(() => {
  const map = new Map<string, PlaySession[]>();
  for (const session of sessions.value) {
    const day = session.startedAt.slice(0, 10);
    if (!map.has(day)) map.set(day, []);
    map.get(day)!.push(session);
  }
  return [...map.entries()].map(([date, items]) => ({
    date,
    items,
    total: items.reduce((sum, s) => sum + s.durationSeconds, 0),
  }));
});

const totalSeconds = computed(() =>
  sessions.value.reduce((sum, s) => sum + s.durationSeconds, 0),
);

async function load() {
  loading.value = true;
  try {
    sessions.value = await statsApi.gameSessions(props.gameId, 300);
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    loading.value = false;
  }
}

async function remove(session: PlaySession) {
  const ok = await confirmDialog({
    title: "删除游玩记录",
    message: `确定删除 ${formatDateTime(session.startedAt)} 的这条记录（${formatDuration(session.durationSeconds)}）吗？\n\n注意：删除记录不会减少游戏的总时长统计。`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;
  try {
    await statsApi.deleteSession(session.id);
    await load();
    toast.success("已删除");
  } catch (error) {
    toast.error(errorText(error));
  }
}

onMounted(load);
</script>

<template>
  <div class="max-w-[860px]">
    <div class="mb-4 flex items-center gap-3">
      <h3 class="text-[14.5px] font-semibold text-ink">游玩记录</h3>
      <span class="text-[13px] text-ink-3">
        {{ sessions.length }} 次 · 合计 {{ formatDuration(totalSeconds) }}
      </span>
      <button class="btn btn-ghost ml-auto h-7 px-2.5 text-[13.5px]" :disabled="loading" @click="load">
        刷新
      </button>
    </div>

    <div v-if="!sessions.length" class="panel px-4 py-10 text-center text-[13.5px] text-ink-3">
      还没有游玩记录。通过 GalManager 启动游戏后会自动开始计时。
    </div>

    <div v-else class="space-y-5">
      <section v-for="group in grouped" :key="group.date">
        <header class="mb-2 flex items-center gap-2.5">
          <span class="text-[13.5px] font-medium text-ink">{{ group.date }}</span>
          <span class="h-px flex-1 bg-line-soft" />
          <span class="text-[12.5px] text-ink-3">{{ formatDuration(group.total) }}</span>
        </header>

        <div class="space-y-1.5">
          <div
            v-for="session in group.items"
            :key="session.id"
            class="panel group flex items-center gap-3 px-3.5 py-2.5"
          >
            <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-accent" />
            <span class="w-[112px] shrink-0 text-[13px] text-ink-3">
              {{ session.startedAt.slice(11, 16) }}
              <template v-if="session.endedAt"> – {{ session.endedAt.slice(11, 16) }}</template>
            </span>
            <span class="flex-1 text-[14px] text-ink-2">
              {{ formatDuration(session.durationSeconds) }}
            </span>
            <button
              class="hidden text-[12.5px] text-ink-3 transition group-hover:block hover:text-danger"
              @click="remove(session)"
            >
              删除
            </button>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
