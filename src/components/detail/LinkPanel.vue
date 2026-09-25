<script setup lang="ts">
import { onMounted, ref } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { extrasApi, type ResourceLink } from "../../api";
import { confirmDialog } from "../../composables/useConfirm";
import { errorText, toast } from "../../utils/toast";
import { formatDateTime } from "../../utils/format";

const props = defineProps<{ gameId: number }>();

const KINDS = [
  { value: "wiki", label: "攻略 Wiki", color: "#e0913c" },
  { value: "patch", label: "补丁下载", color: "#c96a52" },
  { value: "video", label: "视频", color: "#dd5f4a" },
  { value: "forum", label: "论坛讨论", color: "#a9bd6b" },
  { value: "store", label: "商店页面", color: "#e8b04b" },
  { value: "other", label: "其它", color: "#8a7d6d" },
];

const links = ref<ResourceLink[]>([]);
const title = ref("");
const url = ref("");
const kind = ref("wiki");
const remark = ref("");
const adding = ref(false);

async function load() {
  try {
    links.value = await extrasApi.listLinks(props.gameId);
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function add() {
  if (!url.value.trim()) {
    toast.warn("请填写链接地址");
    return;
  }
  adding.value = true;
  try {
    await extrasApi.addLink({
      gameId: props.gameId,
      title: title.value,
      url: url.value.trim(),
      kind: kind.value,
      remark: remark.value || null,
    });
    title.value = "";
    url.value = "";
    remark.value = "";
    toast.success("已收藏链接");
    await load();
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    adding.value = false;
  }
}

async function remove(link: ResourceLink) {
  const ok = await confirmDialog({
    title: "删除链接",
    message: `确定删除「${link.title}」吗？`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;
  try {
    await extrasApi.deleteLink(link.id);
    await load();
    toast.success("已删除");
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function open(link: ResourceLink) {
  try {
    await openUrl(link.url);
  } catch (error) {
    toast.error(errorText(error));
  }
}

const kindMeta = (value: string) =>
  KINDS.find((k) => k.value === value) ?? { label: value, color: "#8a7d6d" };

onMounted(load);
</script>

<template>
  <div class="max-w-[1000px] space-y-4">
    <!-- 添加 -->
    <section class="panel p-4">
      <h3 class="mb-3 text-[13px] font-semibold text-ink">收藏资源链接</h3>
      <div class="grid grid-cols-[1fr_1.6fr_130px_auto] gap-2.5">
        <input v-model="title" class="field" placeholder="标题（留空则用网址）" />
        <input v-model="url" class="field" placeholder="https://…" @keydown.enter="add" />
        <select v-model="kind" class="field cursor-pointer">
          <option v-for="k in KINDS" :key="k.value" :value="k.value">{{ k.label }}</option>
        </select>
        <button class="btn btn-primary" :disabled="adding" @click="add">添加</button>
      </div>
      <input v-model="remark" class="field mt-2.5" placeholder="备注（可选）" />
    </section>

    <!-- 列表 -->
    <div v-if="!links.length" class="panel px-4 py-10 text-center text-[12px] text-ink-3">
      还没有收藏链接。攻略 Wiki、汉化发布页、B站流程视频都可以收在这里。
    </div>

    <div v-else class="space-y-2">
      <div
        v-for="link in links"
        :key="link.id"
        class="panel group flex items-center gap-3 p-3.5"
      >
        <span
          class="h-8 w-8 shrink-0 rounded-lg"
          :style="{ background: kindMeta(link.kind).color + '22', border: `1px solid ${kindMeta(link.kind).color}44` }"
        />
        <div class="min-w-0 flex-1">
          <p class="truncate text-[12.5px] font-medium text-ink">{{ link.title }}</p>
          <p class="mt-0.5 truncate text-[11px] text-ink-3">{{ link.url }}</p>
          <p v-if="link.remark" class="mt-0.5 truncate text-[10.5px] text-ink-3">
            {{ link.remark }}
          </p>
        </div>
        <span class="chip shrink-0" :style="{ color: kindMeta(link.kind).color }">
          {{ kindMeta(link.kind).label }}
        </span>
        <span class="shrink-0 text-[10.5px] text-ink-3">{{ formatDateTime(link.createdAt) }}</span>
        <div class="flex shrink-0 items-center gap-1.5">
          <button class="btn btn-ghost h-7 px-2.5 text-[12px]" @click="open(link)">打开</button>
          <button
            class="btn btn-ghost h-7 px-2.5 text-[12px] border-[#4a2a24] text-danger"
            @click="remove(link)"
          >
            删除
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
