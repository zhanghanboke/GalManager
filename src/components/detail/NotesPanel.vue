<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { extrasApi, type Note } from "../../api";
import { confirmDialog } from "../../composables/useConfirm";
import { errorText, toast } from "../../utils/toast";
import { formatDateTime } from "../../utils/format";

const props = defineProps<{ gameId: number }>();

const notes = ref<Note[]>([]);
const activeId = ref<number | null>(null);
const draftTitle = ref("");
const draftContent = ref("");
const preview = ref(false);
const dirty = ref(false);

const active = computed(() => notes.value.find((n) => n.id === activeId.value) ?? null);

async function load() {
  try {
    notes.value = await extrasApi.listNotes(props.gameId);
    if (notes.value.length && activeId.value === null) {
      select(notes.value[0]);
    }
  } catch (error) {
    toast.error(errorText(error));
  }
}

function select(note: Note) {
  activeId.value = note.id;
  draftTitle.value = note.title;
  draftContent.value = note.content;
  dirty.value = false;
  preview.value = false;
}

function newNote() {
  activeId.value = null;
  draftTitle.value = "";
  draftContent.value = "";
  dirty.value = false;
  preview.value = false;
}

async function save() {
  if (!draftTitle.value.trim() && !draftContent.value.trim()) {
    toast.warn("内容为空");
    return;
  }
  try {
    const id = await extrasApi.saveNote({
      id: activeId.value,
      gameId: props.gameId,
      title: draftTitle.value,
      content: draftContent.value,
    });
    toast.success("笔记已保存");
    await load();
    activeId.value = id;
    dirty.value = false;
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function remove(note: Note) {
  const ok = await confirmDialog({
    title: "删除笔记",
    message: `确定删除「${note.title}」吗？此操作不可撤销。`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;
  try {
    await extrasApi.deleteNote(note.id);
    if (activeId.value === note.id) newNote();
    await load();
    toast.success("已删除");
  } catch (error) {
    toast.error(errorText(error));
  }
}

/** 极简 Markdown 渲染：标题 / 粗体 / 列表 / 分割线，够用且零依赖 */
function renderMarkdown(text: string): string {
  const escape = (s: string) =>
    s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  return escape(text)
    .split("\n")
    .map((line) => {
      if (/^###\s+/.test(line)) return `<h4>${line.replace(/^###\s+/, "")}</h4>`;
      if (/^##\s+/.test(line)) return `<h3>${line.replace(/^##\s+/, "")}</h3>`;
      if (/^#\s+/.test(line)) return `<h2>${line.replace(/^#\s+/, "")}</h2>`;
      if (/^---+$/.test(line.trim())) return "<hr/>";
      if (/^[-*]\s+/.test(line)) return `<li>${line.replace(/^[-*]\s+/, "")}</li>`;
      if (/^\d+\.\s+/.test(line)) return `<li>${line.replace(/^\d+\.\s+/, "")}</li>`;
      if (!line.trim()) return "<p class='sp'></p>";
      return `<p>${line}</p>`;
    })
    .join("")
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/(<li>.*<\/li>)/gs, "<ul>$1</ul>")
    .replace(/<\/ul><ul>/g, "");
}

onMounted(load);
</script>

<template>
  <div class="flex h-full max-w-[1100px] gap-4">
    <!-- 列表 -->
    <aside class="w-[210px] shrink-0">
      <button class="btn btn-primary mb-2.5 w-full text-[12px]" @click="newNote">
        + 新建笔记
      </button>
      <div class="space-y-1">
        <button
          v-for="note in notes"
          :key="note.id"
          class="group flex w-full items-center gap-2 rounded-xl border px-3 py-2 text-left transition"
          :class="
            activeId === note.id
              ? 'border-accent/45 bg-accent/8'
              : 'border-transparent hover:bg-surface-2'
          "
          @click="select(note)"
        >
          <div class="min-w-0 flex-1">
            <p class="truncate text-[12px] text-ink">{{ note.title }}</p>
            <p class="mt-0.5 text-[10.5px] text-ink-3">{{ formatDateTime(note.updatedAt) }}</p>
          </div>
          <span
            class="hidden h-5 w-5 shrink-0 items-center justify-center rounded-md text-ink-3 group-hover:flex hover:text-danger"
            @click.stop="remove(note)"
          >
            <svg width="10" height="10" viewBox="0 0 12 12" fill="none">
              <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
            </svg>
          </span>
        </button>
      </div>
      <p v-if="!notes.length" class="px-1 py-2 text-[11.5px] text-ink-3">
        还没有笔记。攻略、分支路线、CG 解锁条件都可以记在这里。
      </p>
    </aside>

    <!-- 编辑器 -->
    <section class="flex min-w-0 flex-1 flex-col panel p-4">
      <div class="mb-3 flex items-center gap-2">
        <input
          v-model="draftTitle"
          class="field flex-1"
          placeholder="笔记标题，如「全线攻略 / 结局分支」"
          @input="dirty = true"
        />
        <button
          class="btn btn-ghost h-[34px] px-2.5 text-[12px]"
          :class="preview ? 'text-accent' : ''"
          @click="preview = !preview"
        >
          {{ preview ? "编辑" : "预览" }}
        </button>
        <button class="btn btn-primary h-[34px]" :disabled="!dirty && activeId !== null" @click="save">
          保存
        </button>
      </div>

      <textarea
        v-if="!preview"
        v-model="draftContent"
        class="field min-h-[380px] flex-1 font-mono text-[12.5px]"
        placeholder="支持简易 Markdown：&#10;# 一级标题  ## 二级标题  ### 三级标题&#10;- 列表项&#10;**加粗**&#10;--- 分割线"
        @input="dirty = true"
      />

      <div
        v-else
        class="markdown-body min-h-[380px] flex-1 overflow-auto rounded-xl border border-line bg-surface-2 p-4"
        v-html="renderMarkdown(draftContent || '*（空）*')"
      />

      <p v-if="active" class="mt-2 text-[11px] text-ink-3">
        创建于 {{ formatDateTime(active.createdAt) }} · 最后更新 {{ formatDateTime(active.updatedAt) }}
      </p>
    </section>
  </div>
</template>

<style scoped>
.markdown-body :deep(h2) {
  font-size: 16px;
  font-weight: 600;
  color: var(--color-ink);
  margin: 14px 0 8px;
}
.markdown-body :deep(h3) {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-ink);
  margin: 12px 0 6px;
}
.markdown-body :deep(h4) {
  font-size: 13px;
  font-weight: 600;
  color: var(--color-ink-2);
  margin: 10px 0 5px;
}
.markdown-body :deep(p) {
  font-size: 12.5px;
  line-height: 1.75;
  color: var(--color-ink-2);
  margin: 3px 0;
}
.markdown-body :deep(.sp) {
  height: 6px;
}
.markdown-body :deep(ul) {
  margin: 4px 0;
  padding-left: 18px;
  list-style: disc;
}
.markdown-body :deep(li) {
  font-size: 12.5px;
  line-height: 1.75;
  color: var(--color-ink-2);
}
.markdown-body :deep(strong) {
  color: var(--color-ink);
  font-weight: 600;
}
.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--color-line);
  margin: 12px 0;
}
</style>
