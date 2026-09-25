<script setup lang="ts">
import { onMounted, ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  launchApi,
  saveApi,
  settingsApi,
  tagApi,
  transferApi,
  type AutoBackupReport,
  type AutoBackupStatus,
  type ImportOutcome,
} from "../api";
import ImportDialog from "../components/ImportDialog.vue";
import { useLibraryStore } from "../stores/library";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../composables/useConfirm";
import { errorText, toast } from "../utils/toast";
import { colorOf } from "../utils/format";

const settings = useSettingsStore();
const library = useLibraryStore();

const detecting = ref(false);
const backingUp = ref(false);
const busy = ref(false);
const backupReport = ref<[number, string, boolean][]>([]);
const editingTag = ref<number | null>(null);
const tagDraft = ref("");
const exporting = ref(false);
/** 待导入的归档路径；非空时弹出导入确认对话框 */
const importSource = ref<string | null>(null);
const autoStatus = ref<AutoBackupStatus | null>(null);
const autoRunning = ref(false);

/** 自动备份间隔预设（后端下限为 10 分钟） */
const intervalOptions = [
  { value: "30", label: "30 分钟" },
  { value: "60", label: "1 小时" },
  { value: "180", label: "3 小时" },
  { value: "720", label: "12 小时" },
  { value: "1440", label: "24 小时" },
];

const gridOptions = [
  { value: "sm", label: "小" },
  { value: "md", label: "中" },
  { value: "lg", label: "大" },
];

async function update(key: string, value: string) {
  try {
    await settings.set(key, value);
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function detectLe() {
  detecting.value = true;
  try {
    const path = await launchApi.detectLocaleEmulator();
    if (path) {
      await settings.set("le_path", path);
      toast.success(`已检测到 Locale Emulator：${path}`);
    } else {
      toast.warn("未检测到 Locale Emulator，请手动选择 LEProc.exe");
    }
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    detecting.value = false;
  }
}

async function pickLePath() {
  const selected = await open({
    multiple: false,
    title: "选择 LEProc.exe",
    filters: [{ name: "可执行文件", extensions: ["exe"] }],
  });
  if (typeof selected === "string") {
    await settings.set("le_path", selected);
    toast.success("已保存 LE 路径");
  }
}

async function pickBackupRoot() {
  const selected = await open({ directory: true, multiple: false, title: "选择存档备份目录" });
  if (typeof selected === "string") {
    await settings.set("save_backup_root", selected);
    await settings.refreshStorage();
    toast.success("已更新备份目录");
  }
}

async function runBackupAll() {
  const ok = await confirmDialog({
    title: "一键备份全部存档",
    message:
      "将对库中所有能识别到存档目录的游戏执行一次备份。\n\n游戏较多时可能需要一些时间，请勿关闭应用。",
    confirmText: "开始备份",
  });
  if (!ok) return;
  backingUp.value = true;
  try {
    backupReport.value = await saveApi.backupAll();
    const success = backupReport.value.filter(([, , ok]) => ok).length;
    toast.success(`备份完成：成功 ${success} 个，跳过/失败 ${backupReport.value.length - success} 个`);
    await settings.refreshStorage();
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    backingUp.value = false;
  }
}

async function clearCovers() {
  const ok = await confirmDialog({
    title: "清理封面缓存",
    message: "将删除应用数据目录下缓存的封面图片。\n\n注意：自定义封面的路径记录会失效，需要重新设置。",
    confirmText: "清理",
    danger: true,
  });
  if (!ok) return;
  try {
    const count = await settingsApi.clearCoverCache();
    toast.success(`已清理 ${count} 个封面文件`);
    await settings.refreshStorage();
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function optimizeDb() {
  busy.value = true;
  try {
    const message = await settingsApi.optimizeDb();
    toast.success(message);
    await settings.refreshStorage();
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    busy.value = false;
  }
}

async function renameTag(id: number, name: string) {
  if (!name.trim()) return;
  try {
    await tagApi.update(id, name.trim());
    editingTag.value = null;
    await library.refresh();
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function removeTag(id: number, name: string) {
  const ok = await confirmDialog({
    title: "删除标签",
    message: `确定删除标签「${name}」吗？\n所有游戏上的该标签会被一并移除。`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;
  try {
    await tagApi.remove(id);
    await library.refresh();
    toast.success("标签已删除");
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function renameCategory(id: number, name: string) {
  const next = window.prompt("重命名分类", name);
  if (!next || next === name) return;
  try {
    await library.renameCategory(id, next);
    toast.success("已重命名");
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function refreshAutoStatus() {
  try {
    autoStatus.value = await saveApi.autoBackupStatus();
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function onToggleAutoBackup(enabled: boolean) {
  await update("auto_backup_saves", enabled ? "true" : "false");
  await refreshAutoStatus();
}

/** 统一的自动备份结果提示 */
function reportAutoBackup(report: AutoBackupReport) {
  if (report.backedUp > 0) {
    toast.success(
      `自动备份完成：新增 ${report.backedUp} 份（检查 ${report.checked} 个游戏）`,
    );
  } else if (report.failed > 0) {
    toast.warn(`自动备份完成，但有 ${report.failed} 个游戏失败`);
  } else {
    toast.info(`自动备份完成：存档均无变化（检查 ${report.checked} 个游戏）`);
  }
}

async function runAutoBackupNow() {
  autoRunning.value = true;
  try {
    const report = await saveApi.runAutoBackupNow();
    reportAutoBackup(report);
    await settings.refreshStorage();
    await refreshAutoStatus();
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    autoRunning.value = false;
  }
}

/** 导出文件名里的日期戳 */
function dateStamp() {  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`;
}

async function exportLibrary() {
  const target = await save({
    title: "导出游戏库",
    defaultPath: `galmanager-library-${dateStamp()}.json`,
    filters: [{ name: "GalManager 归档", extensions: ["json"] }],
  });
  if (typeof target !== "string") return;

  exporting.value = true;
  try {
    const outcome = await transferApi.export(target);
    const size = outcome.bytes >= 1024 ? `${(outcome.bytes / 1024).toFixed(1)} KB` : `${outcome.bytes} B`;
    toast.success(`已导出 ${outcome.summary.gameCount} 个游戏（${size}）`);
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    exporting.value = false;
  }
}

async function pickImportFile() {
  const selected = await open({
    multiple: false,
    title: "选择 GalManager 归档文件",
    filters: [{ name: "GalManager 归档", extensions: ["json"] }],
  });
  if (typeof selected === "string") {
    importSource.value = selected;
  }
}

async function onImported(outcome: ImportOutcome) {
  importSource.value = null;
  await library.refresh();
  await settings.refreshStorage();
  const skipped = outcome.skippedGames ? `，跳过已存在 ${outcome.skippedGames} 个` : "";
  toast.success(`导入完成：新增 ${outcome.importedGames} 个游戏${skipped}`);
  if (outcome.safetyBackup) {
    toast.info(`导入前的数据库已备份至 ${outcome.safetyBackup}`);
  }
}

onMounted(async () => {
  // 浏览器里没有原生文件对话框，测试时用这个钩子直接打开导入确认框
  if (import.meta.env.DEV) {
    Object.assign(window, {
      __galmanagerOpenImport: (path: string) => {
        importSource.value = path;
      },
    });
  }
  await Promise.all([settings.refreshStorage(), refreshAutoStatus()]);
});
</script>

<template>
  <div class="min-h-0 flex-1 scroll-y px-6 py-5">
    <div class="mx-auto max-w-[880px] space-y-4">
      <header class="mb-1">
        <h1 class="text-[21px] font-semibold tracking-tight text-ink">设置</h1>
        <p class="mt-1 text-[14px] text-ink-3">启动方式、存档备份与数据管理</p>
      </header>

      <!-- 通用 -->
      <section class="panel p-5">
        <h2 class="mb-4 text-[15px] font-semibold text-ink">通用</h2>

        <div class="space-y-4">
          <div>
            <label class="label">封面墙尺寸</label>
            <div class="flex gap-1.5">
              <button
                v-for="option in gridOptions"
                :key="option.value"
                class="btn"
                :class="settings.gridSize === option.value ? 'btn-primary' : 'btn-ghost'"
                @click="update('grid_size', option.value)"
              >
                {{ option.label }}
              </button>
            </div>
          </div>

          <label class="flex cursor-pointer items-start gap-3">
            <input
              type="checkbox"
              class="mt-0.5"
              :checked="settings.values.close_to_tray !== 'false'"
              @change="update('close_to_tray', ($event.target as HTMLInputElement).checked ? 'true' : 'false')"
            />
            <span>
              <span class="text-[14px] text-ink">关闭窗口时最小化到系统托盘</span>
              <span class="mt-0.5 block text-[13px] text-ink-3">
                保持后台运行，游玩计时不中断。托盘图标右键可退出。
              </span>
            </span>
          </label>
        </div>
      </section>

      <!-- 启动 -->
      <section class="panel p-5">
        <h2 class="mb-1 text-[15px] font-semibold text-ink">启动与转区</h2>
        <p class="mb-4 text-[13px] text-ink-3">
          Galgame 多为日文编码，通过 Locale Emulator 启动可避免乱码与区域检测失败。
        </p>

        <div class="space-y-4">
          <div>
            <label class="label">Locale Emulator 路径（LEProc.exe）</label>
            <div class="flex gap-2">
              <input
                class="field flex-1"
                :value="settings.values.le_path ?? ''"
                placeholder="未配置，点击右侧自动检测"
                @change="update('le_path', ($event.target as HTMLInputElement).value)"
              />
              <button class="btn btn-ghost shrink-0" :disabled="detecting" @click="detectLe">
                {{ detecting ? "检测中…" : "自动检测" }}
              </button>
              <button class="btn btn-ghost shrink-0" @click="pickLePath">浏览</button>
            </div>
          </div>

          <div>
            <label class="label">启动参数模板</label>
            <input
              class="field"
              :value="settings.values.le_args_template ?? '{exe}'"
              @change="update('le_args_template', ($event.target as HTMLInputElement).value)"
            />
            <p class="mt-1.5 text-[12.5px] leading-relaxed text-ink-3">
              可用占位符：<code class="text-accent">{exe}</code> 游戏可执行文件 ·
              <code class="text-accent">{args}</code> 游戏参数 ·
              <code class="text-accent">{locale}</code> 区域 ·
              <code class="text-accent">{dir}</code> 工作目录。
              默认 <code class="text-accent">{exe}</code> 即使用 LE 的默认区域配置；
              如需显式指定档案可改为 <code class="text-accent">-runas {locale} {exe}</code>。
            </p>
          </div>

          <div>
            <label class="label">默认转区区域</label>
            <input
              class="field w-[200px]"
              :value="settings.values.default_le_locale ?? 'ja-JP'"
              @change="update('default_le_locale', ($event.target as HTMLInputElement).value)"
            />
          </div>
        </div>
      </section>

      <!-- 存档 -->
      <section class="panel p-5">
        <h2 class="mb-4 text-[15px] font-semibold text-ink">存档备份</h2>

        <div class="space-y-4">
          <div>
            <label class="label">备份根目录</label>
            <div class="flex gap-2">
              <input
                class="field flex-1"
                :value="settings.values.save_backup_root ?? ''"
                placeholder="留空则使用应用数据目录"
                @change="update('save_backup_root', ($event.target as HTMLInputElement).value)"
              />
              <button class="btn btn-ghost shrink-0" @click="pickBackupRoot">浏览</button>
              <button
                class="btn btn-ghost shrink-0"
                @click="launchApi.openBackupDir().catch((e) => toast.error(errorText(e)))"
              >
                打开
              </button>
            </div>
          </div>

          <div class="flex items-center gap-2.5">
            <button class="btn btn-primary" :disabled="backingUp" @click="runBackupAll">
              {{ backingUp ? "备份中…" : "一键备份全部存档" }}
            </button>
            <span class="text-[13px] text-ink-3">
              自动识别各游戏存档位置并逐个备份
            </span>
          </div>

          <div v-if="backupReport.length" class="rounded-xl border border-line bg-surface-2 p-3">
            <p class="mb-2 text-[13px] text-ink-3">最近一次批量备份结果</p>
            <div class="max-h-[180px] scroll-y space-y-1">
              <p
                v-for="[gameId, message, ok] in backupReport"
                :key="gameId"
                class="flex items-center gap-2 text-[13px]"
              >
                <span class="h-1.5 w-1.5 shrink-0 rounded-full" :style="{ background: ok ? '#a9bd6b' : '#e8b04b' }" />
                <span class="text-ink-3">#{{ gameId }}</span>
                <span class="text-ink-2">{{ message }}</span>
              </p>
            </div>
          </div>

          <!-- 定时自动备份 -->
          <div class="border-t border-line-soft pt-4">
            <label class="flex cursor-pointer items-start gap-3">
              <input
                type="checkbox"
                class="mt-0.5"
                :checked="settings.values.auto_backup_saves === 'true'"
                @change="onToggleAutoBackup(($event.target as HTMLInputElement).checked)"
              />
              <span>
                <span class="text-[14px] text-ink">定时自动备份存档</span>
                <span class="mt-0.5 block text-[13px] leading-relaxed text-ink-3">
                  后台按间隔自动备份，只处理存档内容有变化的游戏，
                  不会反复写入完全相同的归档。
                </span>
              </span>
            </label>

            <div v-if="settings.values.auto_backup_saves === 'true'" class="mt-3 space-y-3 pl-6">
              <div>
                <label class="label">备份间隔</label>
                <div class="flex flex-wrap gap-1.5">
                  <button
                    v-for="option in intervalOptions"
                    :key="option.value"
                    class="btn"
                    :class="
                      settings.values.auto_backup_interval_minutes === option.value
                        ? 'btn-primary'
                        : 'btn-ghost'
                    "
                    :aria-pressed="settings.values.auto_backup_interval_minutes === option.value"
                    @click="update('auto_backup_interval_minutes', option.value)"
                  >
                    {{ option.label }}
                  </button>
                </div>
              </div>

              <div class="flex flex-wrap items-center gap-2.5">
                <button class="btn btn-ghost" :disabled="autoRunning" @click="runAutoBackupNow">
                  {{ autoRunning ? "备份中…" : "立即备份一次" }}
                </button>
                <span class="text-[13px] text-ink-3">
                  <template v-if="autoStatus?.lastRunAt">
                    上次执行：{{ autoStatus.lastRunAt }}
                  </template>
                  <template v-else>尚未执行过</template>
                </span>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- 库导出 / 导入 -->
      <section class="panel p-5">
        <h2 class="mb-1 text-[15px] font-semibold text-ink">数据备份与迁移</h2>
        <p class="mb-4 text-[13px] text-ink-3">
          把整个游戏库（游戏、分类、标签、游玩记录、笔记、补丁、资源链接）导出为单个 JSON 文件，
          用于换机迁移，或在数据库损坏后恢复。
        </p>

        <div class="flex flex-wrap gap-2">
          <button class="btn btn-primary" :disabled="exporting" @click="exportLibrary">
            {{ exporting ? "导出中…" : "导出游戏库" }}
          </button>
          <button class="btn btn-ghost" @click="pickImportFile">从归档导入</button>
        </div>

        <p class="mt-3 text-[13px] leading-relaxed text-ink-3">
          归档只包含元数据。封面图片与存档 zip 归档位于应用数据目录，
          换机时需要另外复制 <code class="text-accent">covers/</code> 与
          <code class="text-accent">saves/</code>。
        </p>
      </section>

      <!-- 分类 / 标签管理 -->
      <section class="panel p-5">
        <h2 class="mb-4 text-[15px] font-semibold text-ink">分类与标签管理</h2>

        <div class="grid grid-cols-2 gap-6">
          <div>
            <p class="label">分类（{{ library.categories.length }}）</p>
            <div class="space-y-1">
              <div
                v-for="category in library.categories"
                :key="category.id"
                class="flex items-center gap-2 rounded-lg bg-surface-2 px-3 py-2"
              >
                <span class="h-1.5 w-1.5 rounded-full bg-accent" />
                <span class="min-w-0 flex-1 truncate text-[14px] text-ink">{{ category.name }}</span>
                <span class="text-[12.5px] text-ink-3">{{ category.gameCount }}</span>
                <button
                  class="text-[12.5px] text-ink-3 transition hover:text-accent"
                  @click="renameCategory(category.id, category.name)"
                >
                  改名
                </button>
                <button
                  class="text-[12.5px] text-ink-3 transition hover:text-danger"
                  @click="library.removeCategory(category.id)"
                >
                  删除
                </button>
              </div>
              <p v-if="!library.categories.length" class="py-1 text-[13px] text-ink-3">
                在左侧边栏点 + 新建分类
              </p>
            </div>
          </div>

          <div>
            <p class="label">标签（{{ library.tags.length }}）</p>
            <div class="flex flex-wrap gap-1.5">
              <span
                v-for="tag in library.tags"
                :key="tag.id"
                class="chip group cursor-pointer"
                :style="{ background: colorOf(tag.name) + '1f', color: colorOf(tag.name) }"
              >
                <template v-if="editingTag === tag.id">
                  <input
                    v-model="tagDraft"
                    class="w-[80px] bg-transparent text-[13px] outline-none"
                    autofocus
                    @keydown.enter="renameTag(tag.id, tagDraft)"
                    @blur="editingTag = null"
                  />
                </template>
                <template v-else>
                  <button
                    class="text-[13px]"
                    :aria-label="`重命名标签 ${tag.name}`"
                    :title="`重命名「${tag.name}」`"
                    @click="editingTag = tag.id; tagDraft = tag.name"
                  >
                    {{ tag.name }}
                  </button>
                  <span class="opacity-55" aria-hidden="true">{{ tag.gameCount }}</span>
                  <button
                    class="opacity-0 transition group-hover:opacity-100 focus-visible:opacity-100 group-focus-within:opacity-100"
                    :aria-label="`删除标签 ${tag.name}`"
                    :title="`删除标签「${tag.name}」`"
                    @click="removeTag(tag.id, tag.name)"
                  >
                    <svg width="8" height="8" viewBox="0 0 12 12" fill="none" aria-hidden="true">
                      <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                    </svg>
                  </button>
                </template>
              </span>
              <p v-if="!library.tags.length" class="py-1 text-[13px] text-ink-3">
                还没有标签，可在编辑游戏时添加
              </p>
            </div>
          </div>
        </div>
      </section>

      <!-- 数据 -->
      <section class="panel p-5">
        <h2 class="mb-4 text-[15px] font-semibold text-ink">数据与存储</h2>

        <div v-if="settings.storage" class="mb-4 grid grid-cols-3 gap-3">
          <div class="rounded-xl bg-surface-2 p-3">
            <p class="text-[12.5px] text-ink-3">数据库</p>
            <p class="mt-1 text-[15.5px] font-semibold text-ink">{{ settings.storage.dbSizeHuman }}</p>
          </div>
          <div class="rounded-xl bg-surface-2 p-3">
            <p class="text-[12.5px] text-ink-3">封面缓存</p>
            <p class="mt-1 text-[15.5px] font-semibold text-ink">{{ settings.storage.coversSizeHuman }}</p>
          </div>
          <div class="rounded-xl bg-surface-2 p-3">
            <p class="text-[12.5px] text-ink-3">存档备份</p>
            <p class="mt-1 text-[15.5px] font-semibold text-ink">{{ settings.storage.backupsSizeHuman }}</p>
          </div>
        </div>

        <div class="mb-4 rounded-xl bg-surface-2 p-3">
          <p class="text-[12.5px] text-ink-3">应用数据目录</p>
          <p class="mt-0.5 font-mono text-[13px] break-all text-ink-2">
            {{ settings.storage?.dataDir }}
          </p>
        </div>

        <div class="flex flex-wrap gap-2">
          <button
            class="btn btn-ghost"
            @click="launchApi.openAppDataDir().catch((e) => toast.error(errorText(e)))"
          >
            打开数据目录
          </button>
          <button class="btn btn-ghost" :disabled="busy" @click="optimizeDb">整理数据库</button>
          <button class="btn btn-ghost border-[#4a2a24] text-danger" @click="clearCovers">
            清理封面缓存
          </button>
        </div>
      </section>
    </div>

    <ImportDialog
      v-if="importSource"
      :source-path="importSource"
      @close="importSource = null"
      @done="onImported"
    />
  </div>
</template>
