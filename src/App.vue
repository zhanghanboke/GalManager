<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import AppSidebar from "./components/AppSidebar.vue";
import ConfirmHost from "./components/ConfirmHost.vue";
import TitleBar from "./components/TitleBar.vue";
import ToastHost from "./components/ToastHost.vue";
import { useLibraryStore } from "./stores/library";
import { useSettingsStore } from "./stores/settings";
import { errorText, toast } from "./utils/toast";

const library = useLibraryStore();
const settings = useSettingsStore();
const route = useRoute();
const booting = ref(true);

onMounted(async () => {
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
            <p class="text-[12.5px] text-ink-3">正在准备游戏库…</p>
          </div>
        </div>

        <RouterView v-else v-slot="{ Component }">
          <Transition name="view" mode="out-in">
            <component :is="Component" :key="route.path" />
          </Transition>
        </RouterView>
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
</style>
