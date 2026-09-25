<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import AppSidebar from "./components/AppSidebar.vue";
import ConfirmHost from "./components/ConfirmHost.vue";
import TitleBar from "./components/TitleBar.vue";
import ToastHost from "./components/ToastHost.vue";
import { dragActive, submitDroppedPaths } from "./composables/useDropImport";
import { useLibraryStore } from "./stores/library";
import { useSettingsStore } from "./stores/settings";
import { errorText, toast } from "./utils/toast";

const library = useLibraryStore();
const settings = useSettingsStore();
const route = useRoute();
const router = useRouter();
const booting = ref(true);

/** 收到拖入的路径：切回游戏库页面，由它带着路径打开扫描对话框 */
function handleDroppedPaths(paths: string[]) {
  if (!submitDroppedPaths(paths)) return;
  dragActive.value = false;
  if (route.name !== "library") void router.push("/");
}

let unlistenDragDrop: (() => void) | undefined;

onMounted(async () => {
  // 开发期测试钩子：无头浏览器里没法触发系统级拖放，用它注入假路径走通流程。
  // 放在最前面，避免后面的异步初始化把它挡住。
  if (import.meta.env.DEV) {
    Object.assign(window, {
      __galmanagerSimulateDrop: handleDroppedPaths,
      __galmanagerSimulateDrag: (active: boolean) => {
        dragActive.value = active;
      },
    });
  }

  try {
    await settings.load();
    await settings.bindEvents(() => {
      void library.refreshGames();
    });
    await Promise.all([library.refresh(), settings.refreshRunning()]);
    // 轮询运行态，兜底进程异常退出
    window.setInterval(() => void settings.refreshRunning(), 6000);
  } catch (error) {
    toast.error(`初始化失败：${errorText(error)}`);
  } finally {
    booting.value = false;
  }

  // 系统级文件拖放：把文件夹拖进窗口即可导入
  try {
    unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "enter" || payload.type === "over") {
        dragActive.value = true;
      } else if (payload.type === "leave") {
        dragActive.value = false;
      } else if (payload.type === "drop") {
        handleDroppedPaths(payload.paths ?? []);
      }
    });
  } catch {
    // 浏览器预览等没有 Tauri 运行时的场景，忽略即可
  }
});

onUnmounted(() => {
  try {
    unlistenDragDrop?.();
  } catch {
    // 忽略卸载期的异常
  }
});
</script>

<template>
  <div class="app-shell flex h-screen w-screen flex-col overflow-hidden bg-base">
    <!-- 自绘标题栏：无边框窗口的拖拽区与窗口控制按钮 -->
    <TitleBar />

    <div class="flex min-h-0 flex-1">
      <AppSidebar />

      <main class="relative flex min-w-0 flex-1 flex-col">
        <div v-if="booting" class="flex flex-1 items-center justify-center">
          <div class="flex flex-col items-center gap-3">
            <span
              class="anim-spin h-6 w-6 rounded-full border-2 border-line border-t-accent"
              aria-hidden="true"
            />
            <p class="text-[14px] text-ink-3">正在准备游戏库…</p>
          </div>
        </div>

        <RouterView v-else v-slot="{ Component }">
          <Transition name="view" mode="out-in">
            <component :is="Component" :key="route.path" />
          </Transition>
        </RouterView>

        <!-- 拖拽悬停遮罩 -->
        <Transition name="drop">
          <div
            v-if="dragActive"
            class="pointer-events-none absolute inset-0 z-30 flex items-center justify-center bg-base/72 backdrop-blur-[2px]"
          >
            <div
              class="flex flex-col items-center gap-3 rounded-2xl border-2 border-dashed border-accent/70 bg-surface/85 px-12 py-10 shadow-2xl"
            >
              <svg width="34" height="34" viewBox="0 0 32 32" fill="none" aria-hidden="true">
                <path
                  d="M3 10a2.6 2.6 0 012.6-2.6h6.1l2.4 3.1h12.3A2.6 2.6 0 0129 13.1v11A2.6 2.6 0 0126.4 26.7H5.6A2.6 2.6 0 013 24.1z"
                  stroke="#e0913c"
                  stroke-width="1.9"
                  stroke-linejoin="round"
                />
                <path
                  d="M16 13.4v8M16 21.4l-3.1-3.1M16 21.4l3.1-3.1"
                  stroke="#e0913c"
                  stroke-width="1.9"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
              <p class="text-[15.5px] font-medium text-ink">松手即可添加游戏</p>
              <p class="text-[13.5px] text-ink-3">会自动扫描文件夹并识别游戏与启动程序</p>
            </div>
          </div>
        </Transition>
      </main>
    </div>

    <ToastHost />
    <ConfirmHost />
  </div>
</template>

<style scoped>
.view-enter-active,
.view-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}
.view-enter-from {
  opacity: 0;
  transform: translateY(6px);
}
.view-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

.drop-enter-active,
.drop-leave-active {
  transition: opacity 0.14s ease;
}
.drop-enter-from,
.drop-leave-to {
  opacity: 0;
}
</style>
