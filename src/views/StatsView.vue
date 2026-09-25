<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { statsApi, type DailyPlaytime, type StatsOverview, type YearlyReport } from "../api";
import { useLibraryStore } from "../stores/library";
import { errorText, toast } from "../utils/toast";
import { colorOf, formatDuration, formatHours, yearOptions } from "../utils/format";

const library = useLibraryStore();

const overview = ref<StatsOverview | null>(null);
const report = ref<YearlyReport | null>(null);
const engineDist = ref<[string, number][]>([]);
const daily = ref<DailyPlaytime[]>([]);
const year = ref(new Date().getFullYear());
const years = ref<number[]>(yearOptions());
const loading = ref(false);

const weekLabels = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

const maxMonthly = computed(() => Math.max(1, ...(report.value?.monthly ?? [1])));
const maxWeekday = computed(() => Math.max(1, ...(report.value?.weekday ?? [1])));
const maxRanking = computed(() =>
  Math.max(1, ...(report.value?.ranking ?? []).map((r) => r.seconds)),
);
const maxDaily = computed(() => Math.max(1, ...daily.value.map((d) => d.seconds)));

const engineTotal = computed(() =>
  engineDist.value.reduce((sum, [, count]) => sum + count, 0),
);

/** 近 30 天折线图坐标 */
const dailyPolyline = computed(() => {
  const items = daily.value.slice(-30);
  if (items.length < 2) return "";
  const width = 620;
  const height = 96;
  return items
    .map((item, index) => {
      const x = (index / (items.length - 1)) * width;
      const y = height - (item.seconds / maxDaily.value) * height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
});

const dailyArea = computed(() => {
  if (!dailyPolyline.value) return "";
  return `0,96 ${dailyPolyline.value} 620,96`;
});

const engineLabel = (id: string) =>
  library.categories.length >= 0
    ? ({
        kirikiri: "吉里吉里",
        renpy: "Ren'Py",
        unity: "Unity",
        rpgmaker: "RPG Maker",
        rpgmaker_mv: "RPG Maker MV/MZ",
        nscripter: "NScripter",
        siglus: "SiglusEngine",
        reallive: "RealLive",
        bgi: "BGI",
        artemis: "Artemis",
        wolfrpg: "WOLF RPG",
        unreal: "Unreal",
        other: "未知",
        unknown: "未知",
      }[id] ?? id)
    : id;

async function load() {
  loading.value = true;
  try {
    const [ov, rp, ed, dl, ys] = await Promise.all([
      statsApi.overview(),
      statsApi.yearly(year.value),
      statsApi.engineDistribution(),
      statsApi.daily(30),
      statsApi.playYears(),
    ]);
    overview.value = ov;
    report.value = rp;
    engineDist.value = ed;
    daily.value = dl;
    years.value = ys;
  } catch (error) {
    toast.error(`加载统计失败：${errorText(error)}`);
  } finally {
    loading.value = false;
  }
}

watch(year, load);
onMounted(load);
</script>

<template>
  <div class="min-h-0 flex-1 scroll-y px-6 py-5">
    <div class="mx-auto max-w-[1180px]">
      <header class="mb-5 flex items-end justify-between">
        <div>
          <h1 class="text-[21px] font-semibold tracking-tight text-ink">数据统计</h1>
          <p class="mt-1 text-[14px] text-ink-3">
            记录你在 Galgame 世界里的每一段时光
          </p>
        </div>
        <select v-model.number="year" class="field w-[120px] cursor-pointer">
          <option v-for="y in years" :key="y" :value="y">{{ y }} 年度</option>
        </select>
      </header>

      <!-- 概览卡片 -->
      <section class="mb-5 grid grid-cols-4 gap-3.5">
        <div class="panel p-4">
          <p class="text-[13px] text-ink-3">游戏总数</p>
          <p class="mt-1.5 text-[28px] leading-none font-semibold text-ink">
            {{ overview?.totalGames ?? 0 }}
          </p>
          <p class="mt-2 text-[12.5px] text-ink-3">
            收藏 {{ overview?.favoriteGames ?? 0 }} · 已通关 {{ overview?.completedGames ?? 0 }}
          </p>
        </div>
        <div class="panel p-4">
          <p class="text-[13px] text-ink-3">总游玩时长</p>
          <p class="mt-1.5 text-[28px] leading-none font-semibold text-accent">
            {{ formatHours(overview?.totalPlaySeconds ?? 0) }}
          </p>
          <p class="mt-2 text-[12.5px] text-ink-3">
            近 7 天 {{ formatHours(overview?.weekPlaySeconds ?? 0) }}
          </p>
        </div>
        <div class="panel p-4">
          <p class="text-[13px] text-ink-3">近 30 天</p>
          <p class="mt-1.5 text-[28px] leading-none font-semibold text-sage">
            {{ formatHours(overview?.monthPlaySeconds ?? 0) }}
          </p>
          <p class="mt-2 text-[12.5px] text-ink-3">在玩 {{ overview?.playingGames ?? 0 }} 款</p>
        </div>
        <div class="panel p-4">
          <p class="text-[13px] text-ink-3">存档备份</p>
          <p class="mt-1.5 text-[28px] leading-none font-semibold text-clay">
            {{ overview?.saveSlotCount ?? 0 }}
          </p>
          <p class="mt-2 text-[12.5px] text-ink-3">个槽位</p>
        </div>
      </section>

      <!-- 年度报告 -->
      <section v-if="report" class="panel mb-5 p-5">
        <div class="mb-4 flex items-center gap-3">
          <h2 class="text-[15.5px] font-semibold text-ink">{{ report.year }} 年度游玩报告</h2>
          <span class="chip text-accent">{{ report.totalSessions }} 次游玩</span>
        </div>

        <div class="grid grid-cols-[1.35fr_1fr] gap-6">
          <!-- 月度柱状图 -->
          <div>
            <p class="mb-3 text-[13px] text-ink-3">各月游玩时长分布</p>
            <div class="flex h-[132px] items-end gap-1.5">
              <div
                v-for="(seconds, index) in report.monthly"
                :key="index"
                class="group flex h-full flex-1 flex-col justify-end"
                :title="`${index + 1} 月 · ${formatDuration(seconds)}`"
              >
                <span
                  class="mb-1.5 text-center text-[11px] text-ink-3 opacity-0 transition group-hover:opacity-100"
                >
                  {{ seconds > 0 ? formatHours(seconds) : "" }}
                </span>
                <div
                  class="w-full rounded-t-[3px] transition"
                  :style="{
                    height: `${Math.max((seconds / maxMonthly) * 100, seconds > 0 ? 3 : 1)}%`,
                    background:
                      seconds > 0
                        ? 'linear-gradient(180deg, #eea95e, #c9762c)'
                        : 'var(--color-surface-3)',
                  }"
                />
              </div>
            </div>
            <div class="mt-2 flex gap-1.5">
              <span
                v-for="n in 12"
                :key="n"
                class="flex-1 text-center text-[11px] text-ink-3"
              >
                {{ n }}
              </span>
            </div>
          </div>

          <!-- 关键数据 -->
          <div class="space-y-2.5">
            <div class="flex items-center justify-between rounded-xl bg-surface-2 px-3.5 py-2.5">
              <span class="text-[13.5px] text-ink-3">年度总时长</span>
              <span class="text-[14.5px] font-semibold text-ink">
                {{ formatDuration(report.totalSeconds) }}
              </span>
            </div>
            <div class="flex items-center justify-between rounded-xl bg-surface-2 px-3.5 py-2.5">
              <span class="text-[13.5px] text-ink-3">活跃天数</span>
              <span class="text-[14.5px] font-semibold text-ink">{{ report.activeDays }} 天</span>
            </div>
            <div class="flex items-center justify-between rounded-xl bg-surface-2 px-3.5 py-2.5">
              <span class="text-[13.5px] text-ink-3">日均时长</span>
              <span class="text-[14.5px] font-semibold text-ink">
                {{ formatDuration(report.averageSeconds) }}
              </span>
            </div>
            <div class="flex items-center justify-between rounded-xl bg-surface-2 px-3.5 py-2.5">
              <span class="text-[13.5px] text-ink-3">最长单次</span>
              <span class="text-[14.5px] font-semibold text-ink">
                {{ formatDuration(report.longestSessionSeconds) }}
              </span>
            </div>
            <div class="rounded-xl bg-surface-2 px-3.5 py-2.5">
              <p class="text-[13.5px] text-ink-3">年度最爱</p>
              <p class="mt-0.5 truncate text-[14.5px] font-semibold text-accent">
                {{ report.topGame || "—" }}
              </p>
            </div>
            <div class="flex gap-2.5">
              <div class="flex-1 rounded-xl bg-surface-2 px-3 py-2.5">
                <p class="text-[12.5px] text-ink-3">新通关</p>
                <p class="mt-0.5 text-[16.5px] font-semibold text-sage">
                  {{ report.completedCount }}
                </p>
              </div>
              <div class="flex-1 rounded-xl bg-surface-2 px-3 py-2.5">
                <p class="text-[12.5px] text-ink-3">新入库</p>
                <p class="mt-0.5 text-[16.5px] font-semibold text-clay">
                  {{ report.addedCount }}
                </p>
              </div>
            </div>
          </div>
        </div>

        <!-- 星期分布 -->
        <div class="mt-6">
          <p class="mb-3 text-[13px] text-ink-3">星期分布（你最爱在哪天玩）</p>
          <div class="flex h-[74px] items-end gap-2">
            <div
              v-for="(seconds, index) in report.weekday"
              :key="index"
              class="flex h-full flex-1 flex-col justify-end"
              :title="`${weekLabels[index]} · ${formatDuration(seconds)}`"
            >
              <div
                class="w-full rounded-t-[3px]"
                :style="{
                  height: `${Math.max((seconds / maxWeekday) * 100, seconds > 0 ? 4 : 1)}%`,
                  background: index >= 5 ? '#c96a52' : '#e0913c',
                  opacity: seconds > 0 ? 0.85 : 0.2,
                }"
              />
            </div>
          </div>
          <div class="mt-2 flex gap-2">
            <span v-for="label in weekLabels" :key="label" class="flex-1 text-center text-[11.5px] text-ink-3">
              {{ label }}
            </span>
          </div>
        </div>
      </section>

      <!-- 近 30 天趋势 + 引擎分布 -->
      <section class="mb-5 grid grid-cols-[1.6fr_1fr] gap-4">
        <div class="panel p-4">
          <h2 class="mb-3 text-[14.5px] font-semibold text-ink">近 30 天游玩趋势</h2>
          <svg v-if="daily.length >= 2" viewBox="0 0 620 96" class="h-[110px] w-full" preserveAspectRatio="none">
            <defs>
              <linearGradient id="areaFill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stop-color="#e0913c" stop-opacity="0.34" />
                <stop offset="100%" stop-color="#e0913c" stop-opacity="0" />
              </linearGradient>
            </defs>
            <polygon :points="dailyArea" fill="url(#areaFill)" />
            <polyline
              :points="dailyPolyline"
              fill="none"
              stroke="#e0913c"
              stroke-width="2"
              stroke-linejoin="round"
              stroke-linecap="round"
            />
          </svg>
          <p v-else class="py-8 text-center text-[13.5px] text-ink-3">数据还不够，多玩几天吧</p>
        </div>

        <div class="panel p-4">
          <h2 class="mb-3 text-[14.5px] font-semibold text-ink">引擎分布</h2>
          <div v-if="engineDist.length" class="space-y-2.5">
            <div v-for="[id, count] in engineDist.slice(0, 7)" :key="id">
              <div class="mb-1 flex items-center justify-between text-[13px]">
                <span class="text-ink-2">{{ engineLabel(id) }}</span>
                <span class="text-ink-3">{{ count }}</span>
              </div>
              <div class="h-1.5 overflow-hidden rounded-full bg-surface-3">
                <div
                  class="h-full rounded-full"
                  :style="{
                    width: `${(count / Math.max(engineTotal, 1)) * 100}%`,
                    background: colorOf(id),
                  }"
                />
              </div>
            </div>
          </div>
          <p v-else class="py-8 text-center text-[13.5px] text-ink-3">暂无数据</p>
        </div>
      </section>

      <!-- 时长排行 -->
      <section v-if="report?.ranking.length" class="panel p-4">
        <h2 class="mb-3.5 text-[14.5px] font-semibold text-ink">
          {{ report.year }} 年游玩时长排行
        </h2>
        <div class="space-y-2">
          <div
            v-for="(item, index) in report.ranking.slice(0, 12)"
            :key="item.gameId"
            class="flex items-center gap-3"
          >
            <span
              class="w-5 shrink-0 text-center text-[13.5px] font-semibold"
              :class="index < 3 ? 'text-accent' : 'text-ink-3'"
            >
              {{ index + 1 }}
            </span>
            <div
              class="h-9 w-[27px] shrink-0 overflow-hidden rounded-md border border-line-soft bg-surface-2"
            >
              <img
                v-if="item.coverPath"
                :src="convertFileSrc(item.coverPath)"
                :alt="item.title"
                class="h-full w-full object-cover"
              />
            </div>
            <div class="min-w-0 flex-1">
              <p class="truncate text-[14px] text-ink">{{ item.title }}</p>
              <div class="mt-1 h-1.5 overflow-hidden rounded-full bg-surface-3">
                <div
                  class="h-full rounded-full bg-gradient-to-r from-accent to-clay"
                  :style="{ width: `${(item.seconds / maxRanking) * 100}%` }"
                />
              </div>
            </div>
            <div class="w-[92px] shrink-0 text-right">
              <p class="text-[14px] font-medium text-ink">{{ formatHours(item.seconds) }}</p>
              <p class="text-[12px] text-ink-3">{{ item.sessions }} 次</p>
            </div>
          </div>
        </div>
      </section>

      <div v-if="loading" class="flex justify-center py-10">
        <span class="anim-spin h-5 w-5 rounded-full border-2 border-line border-t-accent" />
      </div>
    </div>
  </div>
</template>
