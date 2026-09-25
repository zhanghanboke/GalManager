<script setup lang="ts">
import { computed, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import BaseModal from "./BaseModal.vue";
import { libraryApi, scanApi, type GameInput, type ScanCandidate } from "../api";
import { useLibraryStore } from "../stores/library";
import { errorText, toast } from "../utils/toast";

const emit = defineEmits<{ close: []; done: [] }>();

const library = useLibraryStore();

const root = ref("");
const maxDepth = ref(3);
const mode = ref<"executable" | "first_level">("executable");
const detectEngine = ref(true);
const scanning = ref(false);
const importing = ref(false);

const candidates = ref<ScanCandidate[]>([]);
const checked = ref<Set<string>>(new Set());
const targetCategory = ref<number | "">("");
const useLocaleEmulator = ref(false);
const scanned = ref(false);

const importable = computed(() =>
  candidates.value.filter((c) => checked.value.has(c.path)),
);

async function pickFolder() {
  const selected = await open({ directory: true, multiple: false, title: "选择游戏根目录" });
  if (typeof selected === "string") root.value = selected;
}

async function runScan() {
  if (!root.value) {
    toast.warn("请先选择要扫描的文件夹");
    return;
  }
  scanning.value = true;
  candidates.value = [];
  checked.value = new Set();
  try {
    const result = await scanApi.scan({
      root: root.value,
      maxDepth: maxDepth.value,
      mode: mode.value,
      detectExecutables: true,
      detectEngine: detectEngine.value,
    });
    candidates.value = result;
    scanned.value = true;
    // 默认勾选「未导入过」的项
    checked.value = new Set(
      result.filter((c) => !c.alreadyImported).map((c) => c.path),
    );
    if (!result.length) {
      toast.info("该目录下没有发现游戏，试试调整扫描模式或加大深度");
    } else {
      toast.success(`发现 ${result.length} 个候选，已默认勾选可导入项`);
    }
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    scanning.value = false;
  }
}

function toggle(path: string) {
  const next = new Set(checked.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  checked.value = next;
}

function toggleAll(value: boolean) {
  checked.value = value
    ? new Set(candidates.value.filter((c) => !c.alreadyImported).map((c) => c.path))
    : new Set();
}

async function doImport() {
  const picked = importable.value;
  if (!picked.length) {
    toast.warn("请至少勾选一个游戏");
    return;
  }
  importing.value = true;
  try {
    const inputs: GameInput[] = await scanApi.buildInputs(picked);
    const withCategory = inputs.map((input) => ({
      ...input,
      categoryId: targetCategory.value === "" ? null : Number(targetCategory.value),
      leLaunch: useLocaleEmulator.value ? 1 : 0,
    }));
    const ids = await libraryApi.importMany(withCategory);
    toast.success(`已导入 ${ids.length} 个游戏`);
    emit("done");
    emit("close");
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    importing.value = false;
  }
}

const engineBadge = (candidate: ScanCandidate) =>
  candidate.engine ? `${candidate.engine} ${candidate.engineConfidence}%` : "未识别";
</script>

<template>
  <BaseModal
    title="添加游戏"
    subtitle="扫描本地文件夹，自动识别游戏目录、启动程序与引擎"
    width="880px"
    :mask-closable="!scanning && !importing"
    @close="emit('close')"
  >
    <!-- 扫描配置 -->
    <section class="space-y-3.5">
      <div>
        <label class="label">游戏根目录</label>
        <div class="flex gap-2">
          <input
            v-model="root"
            class="field flex-1"
            placeholder="例如 D:\Games\Galgame"
            @keydown.enter="runScan"
          />
          <button class="btn btn-ghost" @click="pickFolder">
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
              <path d="M1.5 4a1.5 1.5 0 011.5-1.5h2.4l1.2 1.5H11A1.5 1.5 0 0112.5 5.5v4A1.5 1.5 0 0111 11H3a1.5 1.5 0 01-1.5-1.5z" stroke="currentColor" stroke-width="1.3" />
            </svg>
            浏览
          </button>
        </div>
      </div>

      <div class="grid grid-cols-2 gap-3.5">
        <div>
          <label class="label">扫描模式</label>
          <div class="flex gap-1.5">
            <button
              class="btn flex-1"
              :class="mode === 'executable' ? 'btn-primary' : 'btn-ghost'"
              @click="mode = 'executable'"
            >
              按启动程序
            </button>
            <button
              class="btn flex-1"
              :class="mode === 'first_level' ? 'btn-primary' : 'btn-ghost'"
              @click="mode = 'first_level'"
            >
              按一级目录
            </button>
          </div>
          <p class="mt-1.5 text-[11.5px] leading-relaxed text-ink-3">
            {{
              mode === "executable"
                ? "递归查找含 .exe 的目录，每个目录视为一个游戏，适合目录结构规整的库"
                : "把根目录下每个一级子目录都当作一个游戏，适合「一个游戏一个文件夹」的库"
            }}
          </p>
        </div>

        <div>
          <label class="label">最大递归深度：{{ maxDepth }} 层</label>
          <input
            v-model.number="maxDepth"
            type="range"
            min="1"
            max="6"
            class="w-full "
            :disabled="mode === 'first_level'"
          />
          <label class="mt-2 flex cursor-pointer items-center gap-2 text-[12.5px] text-ink-2">
            <input v-model="detectEngine" type="checkbox" />
            识别引擎类型（Kirikiri / Ren'Py / Unity / RPG Maker …）
          </label>
        </div>
      </div>

      <div class="flex items-center gap-2">
        <button class="btn btn-primary" :disabled="scanning" @click="runScan">
          <span v-if="scanning" class="anim-spin h-3.5 w-3.5 rounded-full border-2 border-white/30 border-t-white" />
          {{ scanning ? "正在扫描…" : "开始扫描" }}
        </button>
        <span v-if="scanned" class="text-[12px] text-ink-3">
          共 {{ candidates.length }} 个候选，已选 {{ checked.size }} 个
        </span>
      </div>
    </section>

    <!-- 结果列表 -->
    <section v-if="scanned && candidates.length" class="mt-4">
      <div class="mb-2 flex items-center gap-3">
        <label class="flex cursor-pointer items-center gap-2 text-[12px] text-ink-2">
          <input
            type="checkbox"
           
            :checked="checked.size > 0 && checked.size === candidates.filter((c) => !c.alreadyImported).length"
            @change="toggleAll(($event.target as HTMLInputElement).checked)"
          />
          全选可导入项
        </label>
      </div>

      <div class="max-h-[300px] overflow-hidden rounded-xl border border-line">
        <div class="scroll-y max-h-[300px]">
          <div
            v-for="candidate in candidates"
            :key="candidate.path"
            class="flex items-center gap-3 border-b border-line-soft px-3 py-2.5 transition last:border-b-0"
            :class="candidate.alreadyImported ? 'opacity-45' : 'hover:bg-surface-2'"
          >
            <input
              type="checkbox"
              class="shrink-0 "
              :checked="checked.has(candidate.path)"
              :disabled="candidate.alreadyImported"
              @change="toggle(candidate.path)"
            />
            <div class="min-w-0 flex-1">
              <p class="truncate text-[12.5px] font-medium text-ink">
                {{ candidate.name }}
                <span v-if="candidate.alreadyImported" class="ml-1.5 text-[11px] font-normal text-ink-3">
                  （已在库中）
                </span>
              </p>
              <p class="truncate text-[11px] text-ink-3" :title="candidate.path">
                {{ candidate.path }}
              </p>
            </div>
            <div class="flex shrink-0 items-center gap-2">
              <span
                v-if="candidate.executables.length"
                class="chip max-w-[160px] truncate"
                :title="candidate.executables.join('\n')"
              >
                {{ candidate.executables[0] }}
                <span v-if="candidate.executables.length > 1" class="opacity-50">
                  +{{ candidate.executables.length - 1 }}
                </span>
              </span>
              <span v-else class="chip text-ink-3">无启动项</span>
              <span
                class="chip"
                :class="candidate.engine ? 'text-accent' : 'text-ink-3'"
                :style="candidate.engine ? { background: '#e0913c1f' } : {}"
              >
                {{ engineBadge(candidate) }}
              </span>
            </div>
          </div>
        </div>
      </div>
    </section>

    <template #footer>
      <div v-if="candidates.length" class="mr-auto flex items-center gap-3">
        <select v-model="targetCategory" class="field h-8 w-[130px] cursor-pointer text-[12px]">
          <option value="">导入到：未分类</option>
          <option v-for="c in library.categories" :key="c.id" :value="c.id">
            导入到：{{ c.name }}
          </option>
        </select>
        <label class="flex cursor-pointer items-center gap-1.5 text-[12px] text-ink-2">
          <input v-model="useLocaleEmulator" type="checkbox" />
          默认转区启动
        </label>
      </div>
      <button class="btn btn-ghost" @click="emit('close')">取消</button>
      <button class="btn btn-primary" :disabled="importing || !importable.length" @click="doImport">
        {{ importing ? "导入中…" : `导入 ${importable.length} 个游戏` }}
      </button>
    </template>
  </BaseModal>
</template>
