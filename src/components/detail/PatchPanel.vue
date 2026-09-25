<script setup lang="ts">
import { onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { extrasApi, launchApi, type Patch } from "../../api";
import BaseModal from "../BaseModal.vue";
import { confirmDialog } from "../../composables/useConfirm";
import { errorText, toast } from "../../utils/toast";
import { formatDateTime } from "../../utils/format";

const props = defineProps<{ gameId: number }>();

const PATCH_TYPES = [
  { value: "translation", label: "汉化补丁" },
  { value: "uncensor", label: "去码补丁" },
  { value: "crack", label: "破解 / 免DVD" },
  { value: "update", label: "官方更新" },
  { value: "other", label: "其它" },
];

const patches = ref<Patch[]>([]);
const dialogOpen = ref(false);
const saving = ref(false);

const form = ref({
  id: null as number | null,
  name: "",
  version: "",
  patchType: "translation",
  filePath: "",
  url: "",
  installed: false,
  remark: "",
});

async function load() {
  try {
    patches.value = await extrasApi.listPatches(props.gameId);
  } catch (error) {
    toast.error(errorText(error));
  }
}

function openCreate() {
  form.value = {
    id: null,
    name: "",
    version: "",
    patchType: "translation",
    filePath: "",
    url: "",
    installed: false,
    remark: "",
  };
  dialogOpen.value = true;
}

function openEdit(patch: Patch) {
  form.value = {
    id: patch.id,
    name: patch.name,
    version: patch.version ?? "",
    patchType: patch.patchType,
    filePath: patch.filePath ?? "",
    url: patch.url ?? "",
    installed: patch.installed === 1,
    remark: patch.remark ?? "",
  };
  dialogOpen.value = true;
}

async function pickFile() {
  const selected = await open({
    multiple: false,
    title: "选择补丁文件或压缩包",
  });
  if (typeof selected === "string") form.value.filePath = selected;
}

async function submit() {
  if (!form.value.name.trim()) {
    toast.warn("请填写补丁名称");
    return;
  }
  saving.value = true;
  try {
    await extrasApi.savePatch({
      id: form.value.id,
      gameId: props.gameId,
      name: form.value.name.trim(),
      version: form.value.version || null,
      patchType: form.value.patchType,
      filePath: form.value.filePath || null,
      url: form.value.url || null,
      installed: form.value.installed,
      remark: form.value.remark || null,
    });
    toast.success(form.value.id ? "已更新补丁信息" : "已添加补丁");
    dialogOpen.value = false;
    await load();
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    saving.value = false;
  }
}

async function toggleInstalled(patch: Patch) {
  try {
    await extrasApi.togglePatch(patch.id, patch.installed !== 1);
    await load();
  } catch (error) {
    toast.error(errorText(error));
  }
}

async function remove(patch: Patch) {
  const ok = await confirmDialog({
    title: "删除补丁记录",
    message: `确定删除「${patch.name}」的记录吗？补丁文件本身不会被删除。`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;
  try {
    await extrasApi.deletePatch(patch.id);
    await load();
    toast.success("已删除");
  } catch (error) {
    toast.error(errorText(error));
  }
}

const typeLabel = (value: string) =>
  PATCH_TYPES.find((t) => t.value === value)?.label ?? value;

onMounted(load);
</script>

<template>
  <div class="max-w-[1000px] space-y-4">
    <div class="flex items-center gap-3">
      <h3 class="text-[13px] font-semibold text-ink">汉化补丁与附加资源</h3>
      <span class="text-[11.5px] text-ink-3">{{ patches.length }} 条记录</span>
      <button class="btn btn-primary ml-auto" @click="openCreate">+ 添加补丁</button>
    </div>

    <div v-if="!patches.length" class="panel px-4 py-10 text-center text-[12px] text-ink-3">
      还没有记录。可以在这里登记汉化补丁、去码补丁、官方更新等信息，
      并标记是否已安装，方便日后重装游戏时快速还原环境。
    </div>

    <div v-else class="space-y-2">
      <div
        v-for="patch in patches"
        :key="patch.id"
        class="panel flex items-center gap-3 p-3.5 transition hover:border-line"
      >
        <button
          class="flex h-5 w-5 shrink-0 items-center justify-center rounded-md border transition"
          :class="patch.installed === 1 ? 'border-sage bg-sage/20' : 'border-line'"
          :title="patch.installed === 1 ? '已安装（点击取消）' : '未安装（点击标记）'"
          @click="toggleInstalled(patch)"
        >
          <svg v-if="patch.installed === 1" width="11" height="11" viewBox="0 0 12 12" fill="none">
            <path d="M2.5 6.2l2.4 2.4L9.5 3.6" stroke="#a9bd6b" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>

        <div class="min-w-0 flex-1">
          <p class="flex items-center gap-2 text-[12.5px] font-medium text-ink">
            {{ patch.name }}
            <span v-if="patch.version" class="text-[11px] font-normal text-ink-3">
              v{{ patch.version }}
            </span>
            <span class="chip" :class="patch.installed === 1 ? 'text-sage' : 'text-ink-3'">
              {{ patch.installed === 1 ? "已安装" : "未安装" }}
            </span>
          </p>
          <p class="mt-0.5 truncate text-[11px] text-ink-3">
            {{ typeLabel(patch.patchType) }}
            <template v-if="patch.filePath"> · {{ patch.filePath }}</template>
            <template v-if="patch.remark"> · {{ patch.remark }}</template>
          </p>
          <p class="mt-0.5 text-[10.5px] text-ink-3">{{ formatDateTime(patch.createdAt) }}</p>
        </div>

        <div class="flex shrink-0 items-center gap-1.5">
          <button
            v-if="patch.filePath"
            class="btn btn-ghost h-7 px-2.5 text-[12px]"
            @click="launchApi.openPath(patch.filePath).catch((e) => toast.error(errorText(e)))"
          >
            打开文件
          </button>
          <a
            v-if="patch.url"
            class="btn btn-ghost h-7 px-2.5 text-[12px]"
            :href="patch.url"
            target="_blank"
            rel="noreferrer"
          >
            访问链接
          </a>
          <button class="btn btn-ghost h-7 px-2.5 text-[12px]" @click="openEdit(patch)">编辑</button>
          <button
            class="btn btn-ghost h-7 px-2.5 text-[12px] border-[#4a2a24] text-danger"
            @click="remove(patch)"
          >
            删除
          </button>
        </div>
      </div>
    </div>

    <BaseModal
      v-if="dialogOpen"
      :title="form.id ? '编辑补丁信息' : '添加补丁'"
      width="560px"
      :mask-closable="!saving"
      @close="dialogOpen = false"
    >
      <div class="space-y-3.5">
        <div>
          <label class="label">名称 *</label>
          <input v-model="form.name" class="field" placeholder="例如 汉化补丁 v1.02" />
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="label">版本</label>
            <input v-model="form.version" class="field" placeholder="1.02" />
          </div>
          <div>
            <label class="label">类型</label>
            <select v-model="form.patchType" class="field cursor-pointer">
              <option v-for="t in PATCH_TYPES" :key="t.value" :value="t.value">{{ t.label }}</option>
            </select>
          </div>
        </div>
        <div>
          <label class="label">本地文件</label>
          <div class="flex gap-2">
            <input v-model="form.filePath" class="field flex-1" placeholder="可选" />
            <button class="btn btn-ghost" @click="pickFile">浏览</button>
          </div>
        </div>
        <div>
          <label class="label">下载链接</label>
          <input v-model="form.url" class="field" placeholder="https://…" />
        </div>
        <div>
          <label class="label">备注</label>
          <input v-model="form.remark" class="field" placeholder="例如：需先安装原版再打补丁" />
        </div>
        <label class="flex cursor-pointer items-center gap-2 text-[12.5px] text-ink-2">
          <input v-model="form.installed" type="checkbox" />
          已安装
        </label>
      </div>

      <template #footer>
        <button class="btn btn-ghost" @click="dialogOpen = false">取消</button>
        <button class="btn btn-primary" :disabled="saving" @click="submit">保存</button>
      </template>
    </BaseModal>
  </div>
</template>
