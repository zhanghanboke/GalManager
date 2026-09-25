<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { convertFileSrc } from "@tauri-apps/api/core";
import GameEditDialog from "../components/GameEditDialog.vue";
import SavePanel from "../components/detail/SavePanel.vue";
import NotesPanel from "../components/detail/NotesPanel.vue";
import PatchPanel from "../components/detail/PatchPanel.vue";
import LinkPanel from "../components/detail/LinkPanel.vue";
import TimelinePanel from "../components/detail/TimelinePanel.vue";
import { libraryApi, launchApi, scanApi, type EngineInfo, type Game } from "../api";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../composables/useConfirm";
import { errorText, toast } from "../utils/toast";
import {
  STATUS_COLORS,
  STATUS_LABELS,
  formatDateTime,
  formatDuration,
  formatRelative,
} from "../utils/format";

const props = defineProps<{ id: string }>();
const router = useRouter();
const settings = useSettingsStore();

const game = ref<Game | null>(null);
const engineInfo = ref<EngineInfo | null>(null);
const saveHint = ref("");
const editing = ref(false);
const activeTab = ref("overview");

const tabs = [
  { key: "overview", label: "概览" },
  { key: "saves", label: "存档" },
  { key: "notes", label: "攻略笔记" },
  { key: "patches", label: "补丁" },
  { key: "links", label: "资源链接" },
  { key: "timeline", label: "游玩记录" },
];

const gameId = computed(() => Number(props.id));
const isRunning = computed(() => (game.value ? settings.runningIds.has(game.value.id) : false));

const coverUrl = computed(() =>
  game.value?.coverPath ? convertFileSrc(game.value.coverPath) : null,
);

const statusColor = computed(() =>
  game.value ? (STATUS_COLORS[game.value.playStatus] ?? "#8a7d6d") : "#8a7d6d",
);

async function load() {
  try {
    game.value = await libraryApi.get(gameId.value);
    const engines = await scanApi.listEngines();
    saveHint.value = engines.find((e) => e.id === game.value?.engine)?.saveHint ?? "";
    if (game.value.path) {
      engineInfo.value = await scanApi.detectEngine(game.value.path);
    }
  } catch (error) {
    toast.error(errorText(error));
    void router.push("/");
  }
}

async function toggleLaunch() {
  if (!game.value) return;
  try {
    if (isRunning.value) {
      await launchApi.stop(game.value.id);
      await settings.refreshRunning();
      toast.info("已停止游戏");
    } else {
      const outcome = await launchApi.launch(game.value.id);
      if (outcome.ok) {
        toast.success(outcome.message);
        await settings.refreshRunning();
      } else {
        toast.warn(outcome.message);
      }
    }
    await load();
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function toggleFavorite() {
  if (!game.value) return;
  const next = game.value.favorite === 1 ? 0 : 1;
  await libraryApi.update(game.value.id, { title: game.value.title, favorite: next });
  await load();
}

async function setStatus(status: string) {
  if (!game.value) return;
  await libraryApi.update(game.value.id, {
    title: game.value.title,
    playStatus: status as Game["playStatus"],
  });
  await load();
}

async function removeGame() {
  if (!game.value) return;
  const ok = await confirmDialog({
    title: "从库中移除",
    message: `确定移除《${game.value.title}》吗？\n\n仅移除记录，不会删除磁盘文件。`,
    confirmText: "移除",
    danger: true,
  });
  if (!ok) return;
  await libraryApi.remove(game.value.id);
  toast.success("已移除");
  void router.push("/");
}

function openPath(path: string | null) {
  if (!path) {
    toast.warn("未设置路径");
    return;
  }
  launchApi.openPath(path).catch((e) => toast.error(errorText(e)));
}

watch(() => props.id, load);
onMounted(load);
</script>

<template>
  <div v-if="game" class="flex min-h-0 flex-1 flex-col">
    <!-- 头部 -->
    <header class="shrink-0 border-b border-line-soft px-6 pt-4 pb-0">
      <button
        class="mb-3 flex items-center gap-1.5 text-[13.5px] text-ink-3 transition hover:text-ink"
        @click="router.push('/')"
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <path d="M7.5 2L3.5 6l4 4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        返回游戏库
      </button>

      <div class="flex gap-5">
        <!-- 封面 -->
        <div
          class="relative w-[122px] shrink-0 overflow-hidden rounded-xl border border-line bg-surface-2"
          style="aspect-ratio: 3 / 4"
        >
          <img v-if="coverUrl" :src="coverUrl" :alt="game.title" class="h-full w-full object-cover" />
          <div v-else class="flex h-full items-center justify-center text-[32px] text-ink-3/40">
            {{ game.title.charAt(0) }}
          </div>
        </div>

        <!-- 信息 -->
        <div class="flex min-w-0 flex-1 flex-col">
          <div class="flex items-start gap-3">
            <div class="min-w-0 flex-1">
              <h1 class="truncate text-[22px] font-semibold tracking-tight text-ink">
                {{ game.title }}
              </h1>
              <p v-if="game.originalTitle" class="mt-0.5 truncate text-[14px] text-ink-3">
                {{ game.originalTitle }}
              </p>
            </div>

            <button
              class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-line transition hover:bg-surface-3"
              :title="game.favorite === 1 ? '取消收藏' : '加入收藏'"
              @click="toggleFavorite"
            >
              <svg width="14" height="14" viewBox="0 0 12 12" :fill="game.favorite === 1 ? '#e8b04b' : 'none'" :stroke="game.favorite === 1 ? '#e8b04b' : '#8a7d6d'" stroke-width="1.2">
                <path d="M6 .8l1.6 3.3 3.6.5-2.6 2.5.6 3.6L6 9l-3.2 1.7.6-3.6L.8 4.6l3.6-.5z" />
              </svg>
            </button>
          </div>

          <!-- 元信息 -->
          <div class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[13.5px] text-ink-3">
            <span class="flex items-center gap-1.5">
              <span class="h-1.5 w-1.5 rounded-full" :style="{ background: statusColor }" />
              {{ STATUS_LABELS[game.playStatus] }}
            </span>
            <span v-if="game.engine" class="text-accent">
              {{ engineInfo?.label || game.engine }}
            </span>
            <span>总时长 {{ formatDuration(game.totalPlaySeconds) }}</span>
            <span>最近 {{ formatRelative(game.lastPlayedAt) }}</span>
            <span v-if="game.developer">{{ game.developer }}</span>
            <span v-if="game.releaseDate">{{ game.releaseDate }}</span>
          </div>

          <!-- 标签 -->
          <div v-if="game.tags.length" class="mt-2.5 flex flex-wrap gap-1.5">
            <span v-for="tag in game.tags" :key="tag.id" class="chip">{{ tag.name }}</span>
          </div>

          <!-- 操作 -->
          <div class="mt-auto flex flex-wrap items-center gap-2 pt-4">
            <button
              class="btn"
              :class="isRunning ? 'btn-danger' : 'btn-primary'"
              @click="toggleLaunch"
            >
              <svg v-if="isRunning" width="12" height="12" viewBox="0 0 14 14" fill="currentColor">
                <rect x="2.5" y="2.5" width="9" height="9" rx="1.6" />
              </svg>
              <svg v-else width="12" height="12" viewBox="0 0 14 14" fill="currentColor">
                <path d="M3.6 2.1l8 4.9-8 4.9z" />
              </svg>
              {{ isRunning ? "停止游戏" : "启动游戏" }}
            </button>

            <button class="btn btn-ghost" @click="openPath(game.path)">打开目录</button>
            <button class="btn btn-ghost" @click="editing = true">编辑信息</button>

            <select
              class="field h-[37px] w-[118px] cursor-pointer"
              :value="game.playStatus"
              @change="setStatus(($event.target as HTMLSelectElement).value)"
            >
              <option v-for="(label, key) in STATUS_LABELS" :key="key" :value="key">
                {{ label }}
              </option>
            </select>

            <button
              class="btn btn-ghost ml-auto border-[#4a2a24] text-danger"
              @click="removeGame"
            >
              移除
            </button>
          </div>
        </div>
      </div>

      <!-- 标签页 -->
      <nav class="mt-4 flex gap-1">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          class="relative px-3.5 py-2.5 text-[14px] transition"
          :class="activeTab === tab.key ? 'text-ink' : 'text-ink-3 hover:text-ink-2'"
          @click="activeTab = tab.key"
        >
          {{ tab.label }}
          <span
            v-if="activeTab === tab.key"
            class="absolute inset-x-2 -bottom-px h-[2px] rounded-full bg-accent"
          />
        </button>
      </nav>
    </header>

    <!-- 内容 -->
    <div class="min-h-0 flex-1 scroll-y px-6 py-5">
      <!-- 概览 -->
      <div v-if="activeTab === 'overview'" class="grid max-w-[1000px] grid-cols-[1fr_280px] gap-5">
        <div class="space-y-4">
          <section class="panel p-4">
            <h3 class="mb-3 text-[14.5px] font-semibold text-ink">游戏简介</h3>
            <p class="text-[14px] leading-relaxed whitespace-pre-line text-ink-2">
              {{ game.description || "暂无简介，可点击「编辑信息」补充。" }}
            </p>
          </section>

          <section v-if="game.notes" class="panel p-4">
            <h3 class="mb-3 text-[14.5px] font-semibold text-ink">备注</h3>
            <p class="text-[14px] leading-relaxed whitespace-pre-line text-ink-2">
              {{ game.notes }}
            </p>
          </section>

          <section class="panel p-4">
            <h3 class="mb-3 text-[14.5px] font-semibold text-ink">游玩概况</h3>
            <div class="grid grid-cols-3 gap-3">
              <div class="rounded-xl bg-surface-2 p-3">
                <p class="text-[12.5px] text-ink-3">总时长</p>
                <p class="mt-1 text-[17.5px] font-semibold text-ink">
                  {{ formatDuration(game.totalPlaySeconds) }}
                </p>
              </div>
              <div class="rounded-xl bg-surface-2 p-3">
                <p class="text-[12.5px] text-ink-3">游玩次数</p>
                <p class="mt-1 text-[17.5px] font-semibold text-ink">{{ game.sessionCount }}</p>
              </div>
              <div class="rounded-xl bg-surface-2 p-3">
                <p class="text-[12.5px] text-ink-3">存档备份</p>
                <p class="mt-1 text-[17.5px] font-semibold text-ink">{{ game.saveCount }}</p>
              </div>
            </div>
          </section>
        </div>

        <aside class="space-y-4">
          <section class="panel p-4">
            <h3 class="mb-3 text-[14.5px] font-semibold text-ink">引擎识别</h3>
            <template v-if="engineInfo">
              <p class="text-[14.5px] font-medium text-accent">{{ engineInfo.label }}</p>
              <p class="mt-1 text-[13px] text-ink-3">置信度 {{ engineInfo.confidence }}%</p>
              <ul class="mt-2.5 space-y-1">
                <li
                  v-for="(item, i) in engineInfo.evidence.slice(0, 4)"
                  :key="i"
                  class="flex gap-1.5 text-[13px] text-ink-3"
                >
                  <span class="text-accent">·</span>{{ item }}
                </li>
              </ul>
            </template>
            <p v-else class="text-[13.5px] text-ink-3">未识别到已知引擎</p>
            <p v-if="saveHint" class="mt-3 rounded-lg bg-surface-2 p-2.5 text-[12.5px] leading-relaxed text-ink-3">
              默认存档位置：{{ saveHint }}
            </p>
          </section>

          <section class="panel p-4">
            <h3 class="mb-3 text-[14.5px] font-semibold text-ink">路径信息</h3>
            <div class="space-y-2.5 text-[13px]">
              <div>
                <p class="text-ink-3">游戏目录</p>
                <p class="mt-0.5 break-all text-ink-2">{{ game.path || "未设置" }}</p>
              </div>
              <div>
                <p class="text-ink-3">启动程序</p>
                <p class="mt-0.5 break-all text-ink-2">{{ game.executable || "未设置" }}</p>
              </div>
              <div>
                <p class="text-ink-3">存档目录</p>
                <p class="mt-0.5 break-all text-ink-2">{{ game.savePath || "自动识别" }}</p>
              </div>
              <div>
                <p class="text-ink-3">转区启动</p>
                <p class="mt-0.5 text-ink-2">
                  {{ game.leLaunch === 1 ? `已启用（${game.leLocale || "ja-JP"}）` : "未启用" }}
                </p>
              </div>
              <div>
                <p class="text-ink-3">入库时间</p>
                <p class="mt-0.5 text-ink-2">{{ formatDateTime(game.createdAt) }}</p>
              </div>
            </div>
          </section>
        </aside>
      </div>

      <SavePanel v-else-if="activeTab === 'saves'" :game="game" />
      <NotesPanel v-else-if="activeTab === 'notes'" :game-id="game.id" />
      <PatchPanel v-else-if="activeTab === 'patches'" :game-id="game.id" />
      <LinkPanel v-else-if="activeTab === 'links'" :game-id="game.id" />
      <TimelinePanel v-else-if="activeTab === 'timeline'" :game-id="game.id" />
    </div>

    <GameEditDialog
      v-if="editing"
      :game="game"
      @close="editing = false"
      @saved="
        () => {
          editing = false;
          load();
        }
      "
    />
  </div>
</template>
