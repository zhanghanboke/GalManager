<script setup lang="ts">
import { onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { launchApi, saveApi, settingsApi, tagApi } from "../api";
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

onMounted(async () => {
  await settings.refreshStorage();
});
</script>

<template>
  <div class="min-h-0 flex-1 scroll-y px-6 py-5">
    <div class="mx-auto max-w-[880px] space-y-4">
      <header class="mb-1">
        <h1 class="text-[19px] font-semibold tracking-tight text-ink">设置</h1>
        <p class="mt-1 text-[12.5px] text-ink-3">启动方式、存档备份与数据管理</p>
      </header>

      <!-- 通用 -->
      <section class="panel p-5">
        <h2 class="mb-4 text-[13.5px] font-semibold text-ink">通用</h2>

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
              class="mt-0.5 accent-[#8b7cf6]"
              :checked="settings.values.close_to_tray !== 'false'"
              @change="update('close_to_tray', ($event.target as HTMLInputElement).checked ? 'true' : 'false')"
            />
            <span>
              <span class="text-[12.5px] text-ink">关闭窗口时最小化到系统托盘</span>
              <span class="mt-0.5 block text-[11.5px] text-ink-3">
                保持后台运行，游玩计时不中断。托盘图标右键可退出。
              </span>
            </span>
          </label>
        </div>
      </section>

      <!-- 启动 -->
      <section class="panel p-5">
        <h2 class="mb-1 text-[13.5px] font-semibold text-ink">启动与转区</h2>
        <p class="mb-4 text-[11.5px] text-ink-3">
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
            <p class="mt-1.5 text-[11px] leading-relaxed text-ink-3">
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
        <h2 class="mb-4 text-[13.5px] font-semibold text-ink">存档备份</h2>

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
            <span class="text-[11.5px] text-ink-3">
              自动识别各游戏存档位置并逐个备份
            </span>
          </div>

          <div v-if="backupReport.length" class="rounded-xl border border-line bg-surface-2 p-3">
            <p class="mb-2 text-[11.5px] text-ink-3">最近一次批量备份结果</p>
            <div class="max-h-[180px] scroll-y space-y-1">
              <p
                v-for="[gameId, message, ok] in backupReport"
                :key="gameId"
                class="flex items-center gap-2 text-[11.5px]"
              >
                <span class="h-1.5 w-1.5 shrink-0 rounded-full" :style="{ background: ok ? '#7dd67d' : '#f0b429' }" />
                <span class="text-ink-3">#{{ gameId }}</span>
                <span class="text-ink-2">{{ message }}</span>
              </p>
            </div>
          </div>
        </div>
      </section>

      <!-- 分类 / 标签管理 -->
      <section class="panel p-5">
        <h2 class="mb-4 text-[13.5px] font-semibold text-ink">分类与标签管理</h2>

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
                <span class="min-w-0 flex-1 truncate text-[12.5px] text-ink">{{ category.name }}</span>
                <span class="text-[11px] text-ink-3">{{ category.gameCount }}</span>
                <button
                  class="text-[11px] text-ink-3 transition hover:text-accent"
                  @click="renameCategory(category.id, category.name)"
                >
                  改名
                </button>
                <button
                  class="text-[11px] text-ink-3 transition hover:text-danger"
                  @click="library.removeCategory(category.id)"
                >
                  删除
                </button>
              </div>
              <p v-if="!library.categories.length" class="py-1 text-[11.5px] text-ink-3">
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
                    class="w-[80px] bg-transparent text-[11.5px] outline-none"
                    autofocus
                    @keydown.enter="renameTag(tag.id, tagDraft)"
                    @blur="editingTag = null"
                  />
                </template>
                <template v-else>
                  <span @click="editingTag = tag.id; tagDraft = tag.name">{{ tag.name }}</span>
                  <span class="opacity-55">{{ tag.gameCount }}</span>
                  <button class="opacity-0 transition group-hover:opacity-100" @click="removeTag(tag.id, tag.name)">
                    <svg width="8" height="8" viewBox="0 0 12 12" fill="none">
                      <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                    </svg>
                  </button>
                </template>
              </span>
              <p v-if="!library.tags.length" class="py-1 text-[11.5px] text-ink-3">
                还没有标签，可在编辑游戏时添加
              </p>
            </div>
          </div>
        </div>
      </section>

      <!-- 数据 -->
      <section class="panel p-5">
        <h2 class="mb-4 text-[13.5px] font-semibold text-ink">数据与存储</h2>

        <div v-if="settings.storage" class="mb-4 grid grid-cols-3 gap-3">
          <div class="rounded-xl bg-surface-2 p-3">
            <p class="text-[11px] text-ink-3">数据库</p>
            <p class="mt-1 text-[14px] font-semibold text-ink">{{ settings.storage.dbSizeHuman }}</p>
          </div>
          <div class="rounded-xl bg-surface-2 p-3">
            <p class="text-[11px] text-ink-3">封面缓存</p>
            <p class="mt-1 text-[14px] font-semibold text-ink">{{ settings.storage.coversSizeHuman }}</p>
          </div>
          <div class="rounded-xl bg-surface-2 p-3">
            <p class="text-[11px] text-ink-3">存档备份</p>
            <p class="mt-1 text-[14px] font-semibold text-ink">{{ settings.storage.backupsSizeHuman }}</p>
          </div>
        </div>

        <div class="mb-4 rounded-xl bg-surface-2 p-3">
          <p class="text-[11px] text-ink-3">应用数据目录</p>
          <p class="mt-0.5 font-mono text-[11.5px] break-all text-ink-2">
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
          <button class="btn btn-ghost border-[#4a2427] text-danger" @click="clearCovers">
            清理封面缓存
          </button>
        </div>
      </section>
    </div>
  </div>
</template>
