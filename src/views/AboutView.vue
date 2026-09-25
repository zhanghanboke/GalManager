<script setup lang="ts">
import { onMounted, ref } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { launchApi } from "../api";
import { useSettingsStore } from "../stores/settings";
import { errorText, toast } from "../utils/toast";

const settings = useSettingsStore();
const revealed = ref(false);

const features = [
  { title: "封面墙游戏库", desc: "自适应网格、多选批量操作、右键快捷菜单，一眼找到想玩的" },
  { title: "自动扫描入库", desc: "递归扫描本地文件夹，自动识别游戏目录、启动程序与引擎类型" },
  { title: "16 种引擎识别", desc: "吉里吉里、Ren'Py、Unity、RPG Maker、SiglusEngine 等" },
  { title: "存档一键备份还原", desc: "按引擎自动定位存档路径，支持多槽位、备注与还原前自动留档" },
  { title: "Locale Emulator 转区", desc: "一键以日文区域启动，告别乱码与区域检测失败" },
  { title: "游玩时长统计", desc: "自动计时、时间线记录、年度游玩报告与时长排行" },
  { title: "攻略笔记与补丁管理", desc: "内嵌 Markdown 笔记、汉化补丁登记、资源链接收藏" },
  { title: "系统托盘常驻", desc: "关闭窗口不中断计时，托盘随时唤回主界面" },
];

const stack = [
  { name: "Tauri 2", role: "桌面应用框架" },
  { name: "Vue 3", role: "前端框架" },
  { name: "TypeScript", role: "类型系统" },
  { name: "Tailwind CSS 4", role: "样式方案" },
  { name: "Rust", role: "后端逻辑" },
  { name: "SQLite", role: "本地数据存储" },
];

function openLink(url: string) {
  openUrl(url).catch((e) => toast.error(errorText(e)));
}

onMounted(() => {
  window.setTimeout(() => (revealed.value = true), 40);
});
</script>

<template>
  <div class="min-h-0 flex-1 scroll-y px-6 py-6">
    <div class="mx-auto max-w-[760px]">
      <!-- 头部 -->
      <section class="mb-6 flex items-start gap-4">
        <div
          class="flex h-16 w-16 shrink-0 items-center justify-center rounded-2xl bg-gradient-to-br from-accent to-clay text-[26px] font-bold text-[#2a1806] shadow-xl shadow-accent/25"
        >
          G
        </div>
        <div class="min-w-0 flex-1">
          <h1 class="text-[22px] font-semibold tracking-tight text-ink">GalManager</h1>
          <p class="mt-1 text-[12.5px] text-ink-3">
            Galgame 专属桌面管理器 · 版本
            <span class="text-accent">v{{ settings.appInfo?.version ?? "0.1.0" }}</span>
          </p>
          <p class="mt-3 max-w-[560px] text-[12.5px] leading-relaxed text-ink-2">
            为 Galgame 玩家打造的一站式本地管理器。把散落在各个硬盘角落的视觉小说整理成一座
            属于自己的封面墙，同时替你记住每一段游玩时光。
          </p>
        </div>
      </section>

      <!-- 功能 -->
      <section class="mb-6">
        <h2 class="mb-3 text-[13.5px] font-semibold text-ink">核心功能</h2>
        <div class="grid grid-cols-2 gap-2.5">
          <div
            v-for="(feature, index) in features"
            :key="feature.title"
            class="panel p-3.5 transition duration-300 hover:border-accent/35"
            :style="{
              opacity: revealed ? 1 : 0,
              transform: revealed ? 'translateY(0)' : 'translateY(10px)',
              transitionDelay: `${index * 40}ms`,
            }"
          >
            <p class="text-[12.5px] font-medium text-ink">{{ feature.title }}</p>
            <p class="mt-1 text-[11.5px] leading-relaxed text-ink-3">{{ feature.desc }}</p>
          </div>
        </div>
      </section>

      <!-- 技术栈 -->
      <section class="panel mb-6 p-4">
        <h2 class="mb-3 text-[13.5px] font-semibold text-ink">技术栈</h2>
        <div class="flex flex-wrap gap-2">
          <span v-for="item in stack" :key="item.name" class="chip">
            <span class="text-ink">{{ item.name }}</span>
            <span class="text-ink-3">{{ item.role }}</span>
          </span>
        </div>
      </section>

      <!-- 环境信息 -->
      <section class="panel mb-6 p-4">
        <h2 class="mb-3 text-[13.5px] font-semibold text-ink">运行环境</h2>
        <dl class="grid grid-cols-2 gap-x-6 gap-y-2.5 text-[12px]">
          <div class="flex justify-between">
            <dt class="text-ink-3">操作系统</dt>
            <dd class="text-ink-2">{{ settings.appInfo?.os }} / {{ settings.appInfo?.arch }}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-ink-3">Tauri 版本</dt>
            <dd class="text-ink-2">{{ settings.appInfo?.tauriVersion }}</dd>
          </div>
          <div class="col-span-2 flex justify-between gap-4">
            <dt class="shrink-0 text-ink-3">数据目录</dt>
            <dd class="truncate font-mono text-[11px] text-ink-2" :title="settings.appInfo?.dataDir">
              {{ settings.appInfo?.dataDir }}
            </dd>
          </div>
        </dl>
        <button
          class="btn btn-ghost mt-3.5 h-7 px-2.5 text-[12px]"
          @click="launchApi.openAppDataDir().catch((e) => toast.error(errorText(e)))"
        >
          打开数据目录
        </button>
      </section>

      <!-- 致谢与链接 -->
      <section class="panel p-4">
        <h2 class="mb-3 text-[13.5px] font-semibold text-ink">致谢</h2>
        <p class="text-[12px] leading-relaxed text-ink-2">
          本项目在架构设计上参考了开源项目
          <button
            class="text-accent hover:underline"
            @click="openLink('https://github.com/huoshen80/ReinaManager')"
          >
            ReinaManager
          </button>
          的模块划分与引擎识别思路，并针对 Vue 3 + Tailwind 技术栈重新实现。
          感谢所有为 Galgame 社区做出贡献的开发者。
        </p>

        <div class="mt-3.5 flex flex-wrap gap-2">
          <button
            class="btn btn-ghost"
            @click="openLink('https://github.com/huoshen80/ReinaManager')"
          >
            ReinaManager 仓库
          </button>
          <button
            class="btn btn-ghost"
            @click="openLink('https://github.com/xupefei/Locale-Emulator')"
          >
            Locale Emulator
          </button>
          <button class="btn btn-ghost" @click="openLink('https://tauri.app')">Tauri</button>
        </div>

        <p class="mt-4 text-[11px] text-ink-3">
          本工具仅用于管理本地已合法获取的游戏，不提供任何游戏内容下载。
        </p>
      </section>
    </div>
  </div>
</template>
