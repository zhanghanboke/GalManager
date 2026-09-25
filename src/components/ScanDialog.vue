<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import BaseModal from "./BaseModal.vue";
import { libraryApi, scanApi, type GameInput, type ScanCandidate } from "../api";
import { useLibraryStore } from "../stores/library";
import { confirmDialog } from "../composables/useConfirm";
import { errorText, toast } from "../utils/toast";

const props = withDefaults(
  defineProps<{
    /** 由拖拽导入等场景预填的根目录；有值时打开即自动扫描 */
    initialPaths?: string[];
  }>(),
  { initialPaths: () => [] },
);

const emit = defineEmits<{ close: []; done: [] }>();

const library = useLibraryStore();

const root = ref("");
/** 额外根目录（拖入多个文件夹时，除第一个外都放这里） */
const extraRoots = ref<string[]>([]);
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

/** 默认勾选的候选：既不在库中，也不是疑似重复 */
const autoImportable = computed(() =>
  candidates.value.filter((c) => !c.alreadyImported && !c.possibleDuplicate),
);

/** 疑似重复的数量（路径不同但标题与库中一致） */
const duplicateCount = computed(
  () => candidates.value.filter((c) => c.possibleDuplicate).length,
);

/** 参与扫描的全部根目录（手动输入的 + 拖入的） */
const allRoots = computed(() =>
  [...new Set([root.value, ...extraRoots.value].map((r) => r.trim()).filter(Boolean))],
);

onMounted(() => {
  const paths = props.initialPaths.map((p) => p.trim()).filter(Boolean);
  if (!paths.length) return;
  root.value = paths[0];
  extraRoots.value = paths.slice(1);
  void runScan();
});

async function pickFolder() {
  const selected = await open({ directory: true, multiple: false, title: "选择游戏根目录" });
  if (typeof selected === "string") root.value = selected;
}

function removeExtraRoot(path: string) {
  extraRoots.value = extraRoots.value.filter((p) => p !== path);
}

async function runScan() {
  const roots = allRoots.value;
  if (!roots.length) {
    toast.warn("请先选择要扫描的文件夹");
    return;
  }
  scanning.value = true;
  candidates.value = [];
  checked.value = new Set();
  try {
    // 多个根目录并发扫描后合并；按路径去重，避免同一游戏出现两条
    const batches = await Promise.all(
      roots.map((r) =>
        scanApi.scan({
          root: r,
          maxDepth: maxDepth.value,
          mode: mode.value,
          detectExecutables: true,
          detectEngine: detectEngine.value,
        }),
      ),
    );
    const seen = new Set<string>();
    const result = batches.flat().filter((c) => {
      if (seen.has(c.path)) return false;
      seen.add(c.path);
      return true;
    });

    candidates.value = result;
    scanned.value = true;
    // 默认只勾选「既不在库中、也不疑似重复」的项
    checked.value = new Set(
      result.filter((c) => !c.alreadyImported && !c.possibleDuplicate).map((c) => c.path),
    );
    if (!result.length) {
      toast.info(
        roots.length > 1
          ? "这些文件夹里没有发现游戏，试试调整扫描模式或加大深度"
          : "该目录下没有发现游戏，试试调整扫描模式或加大深度",
      );
    } else {
      const suspicious = result.filter(
        (c) => c.possibleDuplicate || c.alreadyImported,
      ).length;
      toast.success(
        suspicious
          ? `发现 ${result.length} 个候选，其中 ${suspicious} 个需确认，已默认不勾选`
          : `发现 ${result.length} 个候选，已默认勾选可导入项`,
      );
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
  checked.value = value ? new Set(autoImportable.value.map((c) => c.path)) : new Set();
}

async function doImport() {
  const picked = importable.value;
  if (!picked.length) {
    toast.warn("请至少勾选一个游戏");
    return;
  }

  // 勾了疑似重复项时再确认一次：库里出现两份同一个游戏，
  // 游玩时长与存档备份会各自独立，之后很难合并回来。
  const duplicates = picked.filter((c) => c.possibleDuplicate);
  if (duplicates.length) {
    const lines = duplicates
      .map((c) => `· ${c.name}（库中已有《${c.possibleDuplicate!.title}》）`)
      .join("\n");
    const ok = await confirmDialog({
      title: "确认导入疑似重复的游戏",
      message:
        `以下 ${duplicates.length} 个候选与库中已有游戏标题相同，可能是同一个游戏：\n\n` +
        `${lines}\n\n继续导入会在库中产生两份记录，请确认确实需要分开管理。`,
      confirmText: "仍然导入",
    });
    if (!ok) return;
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
            aria-label="游戏根目录"
            placeholder="例如 D:\Games\Galgame，或直接把文件夹拖进窗口"
            @keydown.enter="runScan"
          />
          <button class="btn btn-ghost" @click="pickFolder">
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <path d="M1.5 4a1.5 1.5 0 011.5-1.5h2.4l1.2 1.5H11A1.5 1.5 0 0112.5 5.5v4A1.5 1.5 0 0111 11H3a1.5 1.5 0 01-1.5-1.5z" stroke="currentColor" stroke-width="1.3" />
            </svg>
            浏览
          </button>
        </div>

        <!-- 拖入的额外目录 -->
        <div v-if="extraRoots.length" class="mt-2 flex flex-wrap items-center gap-1.5">
          <span class="text-[13px] text-ink-3">另外还有 {{ extraRoots.length }} 个拖入的目录：</span>
          <span
            v-for="extra in extraRoots"
            :key="extra"
            class="chip max-w-[280px] gap-1.5"
            :title="extra"
          >
            <span class="truncate">{{ extra }}</span>
            <button
              class="shrink-0 opacity-60 transition hover:opacity-100"
              :aria-label="`移除目录 ${extra}`"
              @click="removeExtraRoot(extra)"
            >
              <svg width="8" height="8" viewBox="0 0 12 12" fill="none" aria-hidden="true">
                <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
              </svg>
            </button>
          </span>
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
          <p class="mt-1.5 text-[13px] leading-relaxed text-ink-3">
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
          <label class="mt-2 flex cursor-pointer items-center gap-2 text-[14px] text-ink-2">
            <input v-model="detectEngine" type="checkbox" />
            识别引擎类型（Kirikiri / Ren'Py / Unity / RPG Maker …）
          </label>
        </div>
      </div>

      <div class="flex items-center gap-2">
        <button class="btn btn-primary" :disabled="scanning" @click="runScan">
          <span v-if="scanning" class="anim-spin h-3.5 w-3.5 rounded-full border-2 border-white/30 border-t-white" />
          {{
            scanning
              ? "正在扫描…"
              : allRoots.length > 1
                ? `开始扫描（${allRoots.length} 个目录）`
                : "开始扫描"
          }}
        </button>
        <span v-if="scanned" class="text-[13.5px] text-ink-3">
          共 {{ candidates.length }} 个候选，已选 {{ checked.size }} 个
        </span>
      </div>
    </section>

    <!-- 结果列表 -->
    <section v-if="scanned && candidates.length" class="mt-4">
      <div class="mb-2 flex flex-wrap items-center gap-3">
        <label class="flex cursor-pointer items-center gap-2 text-[13.5px] text-ink-2">
          <input
            type="checkbox"
            :checked="checked.size > 0 && checked.size === autoImportable.length"
            @change="toggleAll(($event.target as HTMLInputElement).checked)"
          />
          全选可导入项
        </label>
        <span v-if="duplicateCount" class="text-[13px] text-amber">
          {{ duplicateCount }} 个疑似重复已默认不勾选，请核对后再决定
        </span>
      </div>

      <div class="max-h-[300px] overflow-hidden rounded-xl border border-line">
        <div class="scroll-y max-h-[300px]">
          <div
            v-for="candidate in candidates"
            :key="candidate.path"
            class="flex items-center gap-3 border-b border-line-soft px-3 py-2.5 transition last:border-b-0"
            :class="
              candidate.alreadyImported
                ? 'opacity-45'
                : candidate.possibleDuplicate
                  ? 'bg-amber/[0.06] hover:bg-amber/[0.1]'
                  : 'hover:bg-surface-2'
            "
          >
            <input
              type="checkbox"
              class="shrink-0 "
              :checked="checked.has(candidate.path)"
              :disabled="candidate.alreadyImported"
              :aria-label="`选择 ${candidate.name}`"
              @change="toggle(candidate.path)"
            />
            <div class="min-w-0 flex-1">
              <p class="truncate text-[14px] font-medium text-ink">
                {{ candidate.name }}
                <span v-if="candidate.alreadyImported" class="ml-1.5 text-[12.5px] font-normal text-ink-3">
                  （已在库中）
                </span>
                <span
                  v-else-if="candidate.possibleDuplicate"
                  class="ml-1.5 text-[12.5px] font-normal text-amber"
                  :title="`库中已有《${candidate.possibleDuplicate.title}》\n目录：${candidate.possibleDuplicate.path ?? '（未记录）'}`"
                >
                  （可能重复：库中已有《{{ candidate.possibleDuplicate.title }}》）
                </span>
              </p>
              <p class="truncate text-[12.5px] text-ink-3" :title="candidate.path">
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
        <select v-model="targetCategory" class="field h-8 w-[130px] cursor-pointer text-[13.5px]">
          <option value="">导入到：未分类</option>
          <option v-for="c in library.categories" :key="c.id" :value="c.id">
            导入到：{{ c.name }}
          </option>
        </select>
        <label class="flex cursor-pointer items-center gap-1.5 text-[13.5px] text-ink-2">
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
