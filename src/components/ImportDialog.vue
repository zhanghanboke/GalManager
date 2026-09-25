<script setup lang="ts">
/**
 * 库导入确认对话框。
 *
 * 先读取归档摘要让用户看清要导入什么，再选择「合并」或「覆盖」。
 * 覆盖模式属于破坏性操作，界面上明确标红提示。
 */
import { onMounted, ref } from "vue";
import { transferApi, type ArchiveSummary, type ImportOutcome } from "../api";
import { errorText } from "../utils/toast";
import BaseModal from "./BaseModal.vue";

const props = defineProps<{ sourcePath: string }>();
const emit = defineEmits<{ close: []; done: [outcome: ImportOutcome] }>();

const summary = ref<ArchiveSummary | null>(null);
const loading = ref(true);
const importing = ref(false);
const error = ref("");
const mode = ref<"merge" | "replace">("merge");

const statLabels: { key: keyof ArchiveSummary; label: string }[] = [
  { key: "gameCount", label: "游戏" },
  { key: "categoryCount", label: "分类" },
  { key: "tagCount", label: "标签" },
  { key: "sessionCount", label: "游玩记录" },
  { key: "saveSlotCount", label: "存档槽位" },
  { key: "patchCount", label: "补丁" },
  { key: "noteCount", label: "笔记" },
  { key: "linkCount", label: "资源链接" },
];

onMounted(async () => {
  try {
    summary.value = await transferApi.inspect(props.sourcePath);
  } catch (cause) {
    error.value = errorText(cause);
  } finally {
    loading.value = false;
  }
});

async function run() {
  importing.value = true;
  error.value = "";
  try {
    const outcome = await transferApi.import(props.sourcePath, mode.value);
    emit("done", outcome);
  } catch (cause) {
    error.value = errorText(cause);
  } finally {
    importing.value = false;
  }
}
</script>

<template>
  <BaseModal title="从归档导入游戏库" :subtitle="sourcePath" width="600px" @close="emit('close')">
    <div v-if="loading" class="py-6 text-center text-[12.5px] text-ink-3">正在读取归档…</div>

    <div v-else-if="!summary" class="space-y-3">
      <p class="rounded-xl border border-line bg-surface-2 p-3 text-[12px] text-danger">
        {{ error || "无法读取该文件" }}
      </p>
      <div class="flex justify-end">
        <button class="btn btn-ghost" @click="emit('close')">关闭</button>
      </div>
    </div>

    <div v-else class="space-y-4">
      <div class="rounded-xl bg-surface-2 p-3">
        <p class="text-[11px] text-ink-3">归档信息</p>
        <p class="mt-1 text-[12px] text-ink-2">
          导出于 <span class="text-ink">{{ summary.exportedAt }}</span>
          · 应用版本 {{ summary.appVersion || "未知" }}
          · 归档版本 v{{ summary.version }}
        </p>
      </div>

      <div>
        <p class="label">归档内容</p>
        <div class="grid grid-cols-4 gap-2">
          <div
            v-for="item in statLabels"
            :key="item.key"
            class="rounded-lg bg-surface-2 px-3 py-2 text-center"
          >
            <p class="text-[14px] font-semibold text-ink">{{ summary[item.key] }}</p>
            <p class="mt-0.5 text-[10.5px] text-ink-3">{{ item.label }}</p>
          </div>
        </div>
      </div>

      <div>
        <p class="label">导入方式</p>
        <div class="space-y-2">
          <button
            class="w-full rounded-xl border p-3 text-left transition"
            :class="
              mode === 'merge'
                ? 'border-accent bg-surface-3'
                : 'border-line bg-surface-2 hover:bg-surface-3'
            "
            :aria-pressed="mode === 'merge'"
            @click="mode = 'merge'"
          >
            <span class="flex items-center gap-2">
              <span
                class="h-3 w-3 shrink-0 rounded-full border-2"
                :class="mode === 'merge' ? 'border-accent bg-accent' : 'border-line'"
              />
              <span class="text-[12.5px] font-medium text-ink">合并导入（推荐）</span>
            </span>
            <span class="mt-1.5 block pl-5 text-[11.5px] leading-relaxed text-ink-3">
              保留现有游戏库。已存在的游戏（按目录路径判重）会自动跳过，不会产生重复条目。
            </span>
          </button>

          <button
            class="w-full rounded-xl border p-3 text-left transition"
            :class="
              mode === 'replace'
                ? 'border-danger bg-surface-3'
                : 'border-line bg-surface-2 hover:bg-surface-3'
            "
            :aria-pressed="mode === 'replace'"
            @click="mode = 'replace'"
          >
            <span class="flex items-center gap-2">
              <span
                class="h-3 w-3 shrink-0 rounded-full border-2"
                :class="mode === 'replace' ? 'border-danger bg-danger' : 'border-line'"
              />
              <span class="text-[12.5px] font-medium text-danger">覆盖导入</span>
            </span>
            <span class="mt-1.5 block pl-5 text-[11.5px] leading-relaxed text-ink-3">
              先清空当前全部库数据（游戏、分类、标签、笔记等），再导入归档内容。
              导入前会自动备份一份数据库文件。
            </span>
          </button>
        </div>
      </div>

      <p class="rounded-xl border border-line-soft bg-surface-2 p-3 text-[11.5px] leading-relaxed text-ink-3">
        归档只包含元数据。<strong class="text-ink-2">封面图片</strong>与
        <strong class="text-ink-2">存档 zip 归档</strong>存放在应用数据目录，
        换机时需要另外复制 <code class="text-accent">covers/</code> 与
        <code class="text-accent">saves/</code> 目录。
      </p>

      <p v-if="error" class="rounded-xl border border-line bg-surface-2 p-3 text-[12px] text-danger">
        {{ error }}
      </p>
    </div>

    <template v-if="summary" #footer>
      <button class="btn btn-ghost" :disabled="importing" @click="emit('close')">取消</button>
      <button
        class="btn"
        :class="mode === 'replace' ? 'btn-danger' : 'btn-primary'"
        :disabled="importing"
        @click="run"
      >
        {{ importing ? "导入中…" : mode === "replace" ? "清空并导入" : "开始导入" }}
      </button>
    </template>
  </BaseModal>
</template>
