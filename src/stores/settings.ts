/** 应用设置与运行态 */

import { defineStore } from "pinia";
import { computed, ref } from "vue";
import {
  launchApi,
  settingsApi,
  type AppInfo,
  type AutoBackupReport,
  type RunningGame,
  type SettingsMap,
  type StorageInfo,
} from "../api";
import { listen } from "@tauri-apps/api/event";
import { errorText, toast } from "../utils/toast";

export const useSettingsStore = defineStore("settings", () => {
  const values = ref<SettingsMap>({});
  const storage = ref<StorageInfo | null>(null);
  const appInfo = ref<AppInfo | null>(null);
  const loaded = ref(false);

  /** 正在运行的游戏 */
  const running = ref<RunningGame[]>([]);

  const gridSize = computed(() => values.value.grid_size ?? "md");
  /** 游戏库呈现方式：grid = 封面墙，list = 紧凑表格 */
  const viewMode = computed<"grid" | "list">(() =>
    values.value.view_mode === "list" ? "list" : "grid",
  );
  const minimizeToTray = computed(() => values.value.minimize_to_tray !== "false");
  const closeToTray = computed(() => values.value.close_to_tray !== "false");

  const runningIds = computed(() => new Set(running.value.map((r) => r.gameId)));

  async function load() {
    try {
      const [settings, info] = await Promise.all([settingsApi.get(), settingsApi.appInfo()]);
      values.value = settings;
      appInfo.value = info;
      loaded.value = true;
    } catch (error) {
      toast.error(`加载设置失败：${errorText(error)}`);
    }
  }

  async function set(key: string, value: string) {
    values.value[key] = value;
    await settingsApi.set(key, value);
  }

  async function setMany(patch: SettingsMap) {
    Object.assign(values.value, patch);
    await settingsApi.setMany(patch);
  }

  async function refreshStorage() {
    storage.value = await settingsApi.storage();
  }

  async function refreshRunning() {
    running.value = await launchApi.running();
  }

  /** 订阅后端启动 / 退出事件，保持运行态同步 */
  async function bindEvents(onGameExited?: (gameId: number, seconds: number) => void) {
    await listen<RunningGame>("game-launched", () => {
      void refreshRunning();
    });
    await listen<[number, number]>("game-exited", (event) => {
      void refreshRunning();
      const [gameId, seconds] = event.payload;
      if (seconds >= 60) {
        toast.info(`本次游玩 ${Math.round(seconds / 60)} 分钟，已计入统计`);
      }
      onGameExited?.(gameId, seconds);
    });

    // 定时自动备份在后台线程里跑完会广播结果。
    // 只在真的写了新归档或出现失败时打扰用户，全部无变化时保持安静。
    await listen<AutoBackupReport>("auto-backup-finished", (event) => {
      const report = event.payload;
      if (report.backedUp > 0) {
        toast.success(`定时自动备份：新增 ${report.backedUp} 份存档备份`);
      } else if (report.failed > 0) {
        toast.warn(`定时自动备份完成，${report.failed} 个游戏备份失败`);
      }
      void refreshStorage();
    });
  }

  return {
    values,
    storage,
    appInfo,
    loaded,
    running,
    runningIds,
    gridSize,
    viewMode,
    minimizeToTray,
    closeToTray,
    load,
    set,
    setMany,
    refreshStorage,
    refreshRunning,
    bindEvents,
  };
});
