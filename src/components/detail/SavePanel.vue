<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import {
  saveApi,
  launchApi,
  type Game,
  type SavePathProbe,
  type SaveSlot,
} from "../../api";
import { confirmDialog } from "../../composables/useConfirm";
import { errorText, toast } from "../../utils/toast";
import { formatBytes, formatDateTime } from "../../utils/format";

const props = defineProps<{ game: Game }>();

const probe = ref<SavePathProbe | null>(null);
const slots = ref<SaveSlot[]>([]);
const loading = ref(false);
const busy = ref(false);

const selectedPath = ref("");
const slotName = ref("");
const remark = ref("");
const restoreTarget = ref("");
const showInspect = ref<number | null>(null);
const inspectFiles = ref<[string, number][]>([]);

const existingCandidates = computed(
  () => probe.value?.paths.filter((c) => c.exists) ?? [],
);

async function loadProbe() {
  loading.value = true;
  try {
    probe.value = await saveApi.probe(props.game.id);
    const best =
      probe.value.paths.find((c) => c.exists && c.fileCount > 0) ??
      probe.value.paths.find((c) => c.exists);
    if (best && !selectedPath.value) selectedPath.value = best.path;
    restoreTarget.value = selectedPath.value;
  } catch (error) {
    toast.error(`存档路径探测失败：${errorText(error)}`);
  } finally {
    loading.value = false;
  }
}

async function loadSlots() {
  try {
    slots.value = await saveApi.list(props.game.id);
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function pickFolder() {
  const selected = await open({ directory: true, multiple: false, title: "选择存档目录" });
  if (typeof selected === "string") {
    selectedPath.value = selected;
    restoreTarget.value = selected;
  }
}

async function doBackup() {
  if (!selectedPath.value) {
    toast.warn("请先选择要备份的存档目录");
    return;
  }
  busy.value = true;
  try {
    const slot = await saveApi.backup(
      props.game.id,
      selectedPath.value,
      slotName.value || undefined,
      remark.value || undefined,
    );
    toast.success(`已备份 ${slot.fileCount} 个文件（${formatBytes(slot.sizeBytes)}）`);
    slotName.value = "";
    remark.value = "";
    await loadSlots();
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    busy.value = false;
  }
}

async function doRestore(slot: SaveSlot) {
  const target = restoreTarget.value || slot.sourcePath;
  const ok = await confirmDialog({
    title: "还原存档",
    message: `将把「${slot.slotName}」覆盖还原到：\n${target}\n\n还原前会自动把现有存档另存一份，以防误操作。`,
    confirmText: "开始还原",
  });
  if (!ok) return;
  busy.value = true;
  try {
    const outcome = await saveApi.restore(slot.id, target, true);
    toast.success(`已还原 ${outcome.restoredFiles} 个文件`);
    if (outcome.safetyBackup) {
      toast.info(`原存档已另存至 ${outcome.safetyBackup}`);
    }
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    busy.value = false;
  }
}

async function doDelete(slot: SaveSlot) {
  const ok = await confirmDialog({
    title: "删除备份",
    message: `确定删除备份「${slot.slotName}」（${formatDateTime(slot.createdAt)}）吗？\n归档文件也会一并删除。`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;
  try {
    await saveApi.remove(slot.id, true);
    toast.success("已删除备份");
    await loadSlots();
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function inspect(slot: SaveSlot) {
  if (showInspect.value === slot.id) {
    showInspect.value = null;
    return;
  }
  try {
    inspectFiles.value = await saveApi.inspect(slot.id);
    showInspect.value = slot.id;
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function editRemark(slot: SaveSlot) {
  const next = window.prompt("修改备注", slot.remark ?? "");
  if (next === null) return;
  try {
    await saveApi.setRemark(slot.id, next || null);
    await loadSlots();
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function applySavePath(path: string) {
  try {
    await saveApi.setSavePath(props.game.id, path);
    toast.success("已记住该存档目录");
    await loadProbe();
  } catch (error) {
    toast.error(errorText(error));
  }
}

onMounted(async () => {
  await Promise.all([loadProbe(), loadSlots()]);
});
</script>

<template>
  <div class="max-w-[1100px] space-y-4">
    <!-- 探测结果 -->
    <section class="panel p-4">
      <div class="mb-3 flex items-center gap-2.5">
        <h3 class="text-[14.5px] font-semibold text-ink">存档位置</h3>
        <span v-if="probe" class="chip" :class="probe.confidence >= 70 ? 'text-sage' : 'text-ink-3'">
          {{ probe.engineLabel }} · 置信度 {{ probe.confidence }}%
        </span>
        <button class="btn btn-ghost ml-auto h-7 px-2.5 text-[13.5px]" :disabled="loading" @click="loadProbe">
          {{ loading ? "探测中…" : "重新探测" }}
        </button>
      </div>

      <p v-if="probe && !existingCandidates.length" class="mb-3 rounded-lg bg-surface-2 p-3 text-[13.5px] leading-relaxed text-ink-3">
        未自动找到已存在的存档目录。多数 Galgame 需要先启动一次并保存，才会生成存档文件；
        你也可以点击「浏览」手动指定存档位置。
      </p>

      <div class="space-y-1.5">
        <button
          v-for="candidate in probe?.paths ?? []"
          :key="candidate.path"
          class="flex w-full items-center gap-3 rounded-xl border px-3 py-2.5 text-left transition"
          :class="
            selectedPath === candidate.path
              ? 'border-accent/50 bg-accent/8'
              : 'border-line hover:bg-surface-2'
          "
          @click="
            () => {
              selectedPath = candidate.path;
              restoreTarget = candidate.path;
            }
          "
        >
          <span
            class="h-2 w-2 shrink-0 rounded-full"
            :style="{ background: candidate.exists && candidate.fileCount > 0 ? '#a9bd6b' : candidate.exists ? '#e8b04b' : '#4d4033' }"
          />
          <div class="min-w-0 flex-1">
            <p class="truncate text-[13.5px] text-ink">{{ candidate.path }}</p>
            <p class="mt-0.5 text-[12.5px] text-ink-3">{{ candidate.source }}</p>
          </div>
          <span v-if="candidate.exists" class="shrink-0 text-[12.5px] text-ink-3">
            {{ candidate.fileCount }} 个文件 · {{ formatBytes(candidate.sizeBytes) }}
          </span>
          <span v-else class="shrink-0 text-[12.5px] text-ink-3">不存在</span>
        </button>
      </div>

      <div class="mt-3 flex gap-2">
        <button class="btn btn-ghost h-7 px-2.5 text-[13.5px]" @click="pickFolder">浏览…</button>
        <button
          class="btn btn-ghost h-7 px-2.5 text-[13.5px]"
          :disabled="!selectedPath"
          @click="applySavePath(selectedPath)"
        >
          记住此目录
        </button>
        <button
          class="btn btn-ghost h-7 px-2.5 text-[13.5px]"
          :disabled="!selectedPath"
          @click="launchApi.openPath(selectedPath).catch((e) => toast.error(errorText(e)))"
        >
          在资源管理器中打开
        </button>
      </div>
    </section>

    <!-- 备份 -->
    <section class="panel p-4">
      <h3 class="mb-3 text-[14.5px] font-semibold text-ink">新建备份</h3>
      <div class="grid grid-cols-[1fr_1fr_auto] gap-2.5">
        <div>
          <label class="label">槽位名称</label>
          <input v-model="slotName" class="field" placeholder="留空则使用「自动备份」" />
        </div>
        <div>
          <label class="label">备注</label>
          <input v-model="remark" class="field" placeholder="例如：真结局前 / 全 CG 存档" />
        </div>
        <div class="flex items-end">
          <button class="btn btn-primary" :disabled="busy || !selectedPath" @click="doBackup">
            {{ busy ? "备份中…" : "一键备份" }}
          </button>
        </div>
      </div>
    </section>

    <!-- 备份列表 -->
    <section class="panel overflow-hidden">
      <header class="flex items-center justify-between border-b border-line-soft px-4 py-3">
        <h3 class="text-[14.5px] font-semibold text-ink">
          备份槽位
          <span class="ml-1.5 font-normal text-ink-3">{{ slots.length }}</span>
        </h3>
        <div class="flex items-center gap-2">
          <span class="text-[13px] text-ink-3">还原目标：</span>
          <input
            v-model="restoreTarget"
            class="field h-7 w-[260px] text-[13px]"
            placeholder="留空则还原到原目录"
          />
        </div>
      </header>

      <div v-if="!slots.length" class="px-4 py-8 text-center text-[13.5px] text-ink-3">
        还没有备份，先创建一个吧
      </div>

      <div v-else>
        <div
          v-for="slot in slots"
          :key="slot.id"
          class="border-b border-line-soft last:border-b-0"
        >
          <div class="flex items-center gap-3 px-4 py-3 transition hover:bg-surface-2/50">
            <div class="min-w-0 flex-1">
              <p class="flex items-center gap-2 text-[14px] font-medium text-ink">
                {{ slot.slotName }}
                <span v-if="slot.remark" class="font-normal text-ink-3">· {{ slot.remark }}</span>
              </p>
              <p class="mt-0.5 truncate text-[12.5px] text-ink-3">
                {{ formatDateTime(slot.createdAt) }} · {{ slot.fileCount }} 个文件 ·
                {{ formatBytes(slot.sizeBytes) }}
              </p>
            </div>
            <div class="flex shrink-0 items-center gap-1.5">
              <button class="btn btn-ghost h-7 px-2.5 text-[13.5px]" @click="inspect(slot)">
                {{ showInspect === slot.id ? "收起" : "查看内容" }}
              </button>
              <button class="btn btn-ghost h-7 px-2.5 text-[13.5px]" @click="editRemark(slot)">
                备注
              </button>
              <button class="btn btn-primary h-7 px-2.5 text-[13.5px]" @click="doRestore(slot)">
                一键还原
              </button>
              <button
                class="btn btn-ghost h-7 px-2.5 text-[13.5px] border-[#4a2a24] text-danger"
                @click="doDelete(slot)"
              >
                删除
              </button>
            </div>
          </div>

          <div v-if="showInspect === slot.id" class="border-t border-line-soft bg-base/40 px-4 py-3">
            <p class="mb-2 text-[12.5px] text-ink-3">归档内含 {{ inspectFiles.length }} 个文件</p>
            <div class="max-h-[180px] scroll-y rounded-lg bg-surface-2 p-2.5">
              <p
                v-for="([name, size], i) in inspectFiles.slice(0, 200)"
                :key="i"
                class="flex justify-between gap-3 py-0.5 font-mono text-[12.5px] text-ink-3"
              >
                <span class="truncate">{{ name }}</span>
                <span class="shrink-0">{{ formatBytes(size) }}</span>
              </p>
            </div>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>
