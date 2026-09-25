<script setup lang="ts">
/**
 * 自绘标题栏。
 *
 * 窗口在 tauri.conf.json 中设为 `decorations: false`，系统边框与标题栏全部移除，
 * 由本组件提供品牌区、拖拽区与右上角的「最小化 / 最大化 / 关闭」按钮。
 *
 * 关闭按钮走的是 `close()`，会触发 Rust 侧 `CloseRequested`，
 * 因此「关闭时最小化到托盘」的设置依然生效。
 */
import { computed, onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useRoute } from "vue-router";

const appWindow = getCurrentWindow();
const route = useRoute();

const maximized = ref(false);
let unlisten: (() => void) | null = null;

/** 当前页面名，显示在标题栏中部作为轻量面包屑（游戏库首页不显示） */
const PAGE_TITLES: Record<string, string> = {
  stats: "数据统计",
  settings: "设置",
  about: "关于",
  game: "游戏详情",
};
const pageTitle = computed(() => PAGE_TITLES[String(route.name ?? "")] ?? "");

onMounted(async () => {
  try {
    maximized.value = await appWindow.isMaximized();
    // 拖拽到屏幕边缘贴靠、双击等系统行为也会改变最大化状态，这里同步图标
    unlisten = await appWindow.onResized(async () => {
      try {
        maximized.value = await appWindow.isMaximized();
      } catch {
        /* 忽略：窗口可能正在销毁 */
      }
    });
  } catch {
    /* 非 Tauri 环境（如浏览器预览）下静默降级 */
  }
});

onUnmounted(() => {
  try {
    unlisten?.();
  } catch {
    /* 忽略：非 Tauri 环境下 unlisten 可能不是函数 */
  }
});

/**
 * 拖拽 / 双击最大化。
 * 参考 Tauri 官方自定义标题栏方案：`data-tauri-drag-region` 只对直接命中的元素生效，
 * 因此这里用显式事件处理，顺带排除按钮等交互元素。
 */
function onTitlebarMouseDown(event: MouseEvent) {
  if (event.button !== 0) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest("button, a, input, select, textarea, [data-no-drag]")) return;

  if (event.detail === 2) {
    void toggleMaximize();
  } else {
    void appWindow.startDragging().catch(() => undefined);
  }
}

function minimize() {
  void appWindow.minimize().catch(() => undefined);
}

function toggleMaximize() {
  void appWindow.toggleMaximize().catch(() => undefined);
}

function close() {
  void appWindow.close().catch(() => undefined);
}
</script>

<template>
  <header
    class="titlebar relative z-30 flex h-[38px] shrink-0 items-center select-none"
    @mousedown="onTitlebarMouseDown"
    @contextmenu.prevent
  >
    <!-- 品牌 -->
    <div class="flex items-center gap-2.5 pl-3.5">
      <div
        class="flex h-[22px] w-[22px] items-center justify-center rounded-[7px] bg-gradient-to-br from-accent to-rose text-[12px] font-bold text-white shadow-sm shadow-accent/30"
      >
        G
      </div>
      <span class="text-[12.5px] font-semibold tracking-tight text-ink">GalManager</span>
      <template v-if="pageTitle">
        <span class="text-ink-3/40">/</span>
        <span class="text-[11.5px] text-ink-3">{{ pageTitle }}</span>
      </template>
    </div>

    <!-- 拖拽留白 -->
    <div class="h-full flex-1" />

    <!-- 窗口控制 -->
    <div class="flex h-full" data-no-drag>
      <button class="tb-btn" title="最小化" @click="minimize">
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
          <path d="M0 5h10" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>

      <button
        class="tb-btn"
        :title="maximized ? '向下还原' : '最大化'"
        @click="toggleMaximize"
      >
        <svg v-if="maximized" width="10" height="10" viewBox="0 0 10 10" fill="none">
          <path
            d="M2.5 2.5V.5h7v7h-2"
            stroke="currentColor"
            stroke-width="1"
            stroke-linejoin="round"
          />
          <rect
            x=".5"
            y="2.5"
            width="7"
            height="7"
            rx=".6"
            stroke="currentColor"
            stroke-width="1"
          />
        </svg>
        <svg v-else width="10" height="10" viewBox="0 0 10 10" fill="none">
          <rect
            x=".5"
            y=".5"
            width="9"
            height="9"
            rx=".8"
            stroke="currentColor"
            stroke-width="1"
          />
        </svg>
      </button>

      <button class="tb-btn tb-btn-close" title="关闭" @click="close">
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
          <path
            d="M.6.6l8.8 8.8M9.4.6L.6 9.4"
            stroke="currentColor"
            stroke-width="1.05"
            stroke-linecap="round"
          />
        </svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  background: linear-gradient(180deg, #14171f 0%, #101319 100%);
  border-bottom: 1px solid var(--color-line-soft);
  /* 顶部一丝高光，让标题栏与内容区分层更自然 */
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.03);
}

.tb-btn {
  display: inline-flex;
  width: 46px;
  height: 100%;
  align-items: center;
  justify-content: center;
  color: var(--color-ink-2);
  transition: background 0.14s ease, color 0.14s ease;
}
.tb-btn:hover {
  background: var(--color-surface-3);
  color: var(--color-ink);
}
.tb-btn:active {
  background: var(--color-line);
}
.tb-btn-close:hover {
  background: #c42b1c;
  color: #fff;
}
.tb-btn-close:active {
  background: #b1271a;
}
</style>
