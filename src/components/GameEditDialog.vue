<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import BaseModal from "./BaseModal.vue";
import {
  libraryApi,
  scanApi,
  type Game,
  type GameInput,
  type PlayStatus,
} from "../api";
import { useLibraryStore } from "../stores/library";
import { errorText, toast } from "../utils/toast";
import { STATUS_LABELS } from "../utils/format";

const props = defineProps<{ game?: Game | null; defaultPath?: string }>();
const emit = defineEmits<{ close: []; saved: [] }>();

const library = useLibraryStore();
const isEdit = computed(() => Boolean(props.game?.id));

const form = ref({
  title: props.game?.title ?? "",
  originalTitle: props.game?.originalTitle ?? "",
  path: props.game?.path ?? props.defaultPath ?? "",
  executable: props.game?.executable ?? "",
  args: props.game?.args ?? "",
  engine: props.game?.engine ?? "",
  categoryId: props.game?.categoryId ?? (null as number | null),
  playStatus: (props.game?.playStatus ?? "unplayed") as PlayStatus,
  rating: props.game?.rating ?? -1,
  leLaunch: (props.game?.leLaunch ?? 0) === 1,
  leLocale: props.game?.leLocale ?? "ja-JP",
  releaseDate: props.game?.releaseDate ?? "",
  developer: props.game?.developer ?? "",
  description: props.game?.description ?? "",
  notes: props.game?.notes ?? "",
  savePath: props.game?.savePath ?? "",
  tagsText: (props.game?.tags ?? []).map((t) => t.name).join(", "),
});

const coverPath = ref(props.game?.coverPath ?? "");
const coverUrl = computed(() => (coverPath.value ? convertFileSrc(coverPath.value) : null));
const executables = ref<string[]>([]);
const detecting = ref(false);
const saving = ref(false);
const engineOptions = ref<{ id: string; label: string }[]>([]);

const statuses: PlayStatus[] = ["unplayed", "playing", "completed", "on_hold", "dropped"];

onMounted(async () => {
  engineOptions.value = await scanApi.listEngines();
  if (form.value.path) await loadExecutables();
});

async function pickFolder() {
  const selected = await open({ directory: true, multiple: false, title: "选择游戏目录" });
  if (typeof selected !== "string") return;
  form.value.path = selected;
  await loadExecutables();
  if (!form.value.title) {
    const parts = selected.split(/[\\/]/).filter(Boolean);
    form.value.title = parts[parts.length - 1] ?? "";
  }
  await detect();
}

async function loadExecutables() {
  try {
    executables.value = await scanApi.listExecutables(form.value.path);
    if (!form.value.executable && executables.value.length) {
      form.value.executable = executables.value[0];
    }
  } catch {
    executables.value = [];
  }
}

async function detect() {
  if (!form.value.path) return;
  detecting.value = true;
  try {
    const info = await scanApi.detectEngine(form.value.path);
    if (info) {
      form.value.engine = info.id;
      toast.info(`识别为 ${info.label}（置信度 ${info.confidence}%）`);
    } else {
      toast.warn("未能识别引擎类型，可手动选择");
    }
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    detecting.value = false;
  }
}

async function pickCover() {
  const selected = await open({
    multiple: false,
    title: "选择封面图片",
    filters: [{ name: "图片", extensions: ["jpg", "jpeg", "png", "webp", "bmp"] }],
  });
  if (typeof selected !== "string") return;
  coverPath.value = selected; // 保存时再复制进应用目录
}

async function submit() {
  if (!form.value.title.trim()) {
    toast.warn("游戏名不能为空");
    return;
  }
  saving.value = true;
  try {
    const tags = form.value.tagsText
      .split(/[,，\s]+/)
      .map((s) => s.trim())
      .filter(Boolean);

    const input: GameInput = {
      title: form.value.title.trim(),
      originalTitle: form.value.originalTitle || null,
      path: form.value.path || null,
      executable: form.value.executable || null,
      args: form.value.args || null,
      engine: form.value.engine || null,
      categoryId: form.value.categoryId,
      playStatus: form.value.playStatus,
      rating: form.value.rating,
      leLaunch: form.value.leLaunch ? 1 : 0,
      leLocale: form.value.leLocale || null,
      releaseDate: form.value.releaseDate || null,
      developer: form.value.developer || null,
      description: form.value.description || null,
      notes: form.value.notes || null,
      savePath: form.value.savePath || null,
      tags,
    };

    let gameId = props.game?.id ?? 0;
    if (isEdit.value) {
      await libraryApi.update(gameId, input);
    } else {
      gameId = await libraryApi.create(input);
    }

    // 封面：本地新选的图片需要复制到应用数据目录
    if (coverPath.value && coverPath.value !== props.game?.coverPath) {
      await libraryApi.setCover(gameId, coverPath.value);
    }

    toast.success(isEdit.value ? "已保存修改" : "已添加到游戏库");
    emit("saved");
    emit("close");
  } catch (error) {
    toast.error(errorText(error));
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <BaseModal
    :title="isEdit ? '编辑游戏信息' : '手动添加游戏'"
    :subtitle="isEdit ? form.title : '手动填写游戏信息，或先扫描再补充'"
    width="720px"
    :mask-closable="!saving"
    @close="emit('close')"
  >
    <div class="grid grid-cols-[150px_1fr] gap-5">
      <!-- 封面 -->
      <div>
        <label class="label">封面</label>
        <div
          class="group relative overflow-hidden rounded-xl border border-line bg-surface-2"
          style="aspect-ratio: 3 / 4"
        >
          <img
            v-if="coverUrl"
            :src="coverUrl"
            alt="封面预览"
            class="h-full w-full object-cover"
          />
          <div v-else class="flex h-full items-center justify-center text-[11.5px] text-ink-3">
            未设置封面
          </div>
          <button
            class="absolute inset-x-0 bottom-0 bg-black/65 py-2 text-[11.5px] text-white opacity-0 transition group-hover:opacity-100"
            @click="pickCover"
          >
            选择图片
          </button>
        </div>
        <button class="btn btn-ghost mt-2 w-full text-[12px]" @click="pickCover">更换封面</button>
      </div>

      <!-- 表单 -->
      <div class="space-y-3.5">
        <div>
          <label class="label">游戏名 *</label>
          <input v-model="form.title" class="field" placeholder="例如 千恋万花" />
        </div>

        <div>
          <label class="label">原名</label>
          <input v-model="form.originalTitle" class="field" placeholder="日文 / 英文原名，可选" />
        </div>

        <div>
          <label class="label">游戏目录</label>
          <div class="flex gap-2">
            <input v-model="form.path" class="field flex-1" placeholder="D:\Games\SenrenBanka" />
            <button class="btn btn-ghost" @click="pickFolder">浏览</button>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="label">启动程序</label>
            <select v-if="executables.length" v-model="form.executable" class="field cursor-pointer">
              <option v-for="exe in executables" :key="exe" :value="exe">{{ exe }}</option>
            </select>
            <input v-else v-model="form.executable" class="field" placeholder="senren.exe" />
          </div>
          <div>
            <label class="label">引擎</label>
            <div class="flex gap-2">
              <select v-model="form.engine" class="field cursor-pointer">
                <option value="">未识别</option>
                <option v-for="engine in engineOptions" :key="engine.id" :value="engine.id">
                  {{ engine.label }}
                </option>
              </select>
              <button class="btn btn-ghost shrink-0 px-2.5" :disabled="detecting" @click="detect">
                {{ detecting ? "…" : "重测" }}
              </button>
            </div>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="label">分类</label>
            <select v-model="form.categoryId" class="field cursor-pointer">
              <option :value="null">未分类</option>
              <option v-for="c in library.categories" :key="c.id" :value="c.id">{{ c.name }}</option>
            </select>
          </div>
          <div>
            <label class="label">状态</label>
            <select v-model="form.playStatus" class="field cursor-pointer">
              <option v-for="s in statuses" :key="s" :value="s">{{ STATUS_LABELS[s] }}</option>
            </select>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="label">开发商</label>
            <input v-model="form.developer" class="field" placeholder="可选" />
          </div>
          <div>
            <label class="label">发售日期</label>
            <input v-model="form.releaseDate" class="field" placeholder="2020-05-29" />
          </div>
        </div>

        <div>
          <label class="label">标签（逗号分隔）</label>
          <input v-model="form.tagsText" class="field" placeholder="纯爱, 校园, 汉化" />
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="label">启动参数</label>
            <input v-model="form.args" class="field" placeholder="可选，如 -window" />
          </div>
          <div>
            <label class="label">评分</label>
            <select v-model.number="form.rating" class="field cursor-pointer">
              <option :value="-1">未评分</option>
              <option v-for="n in 10" :key="n" :value="n * 10">{{ n * 10 }} 分</option>
            </select>
          </div>
        </div>

        <div class="rounded-xl border border-line bg-surface-2 p-3">
          <label class="flex cursor-pointer items-center gap-2 text-[12.5px] text-ink">
            <input v-model="form.leLaunch" type="checkbox" />
            使用 Locale Emulator 转区启动
          </label>
          <div v-if="form.leLaunch" class="mt-2.5">
            <label class="label">转区区域</label>
            <input v-model="form.leLocale" class="field" placeholder="ja-JP" />
            <p class="mt-1.5 text-[11px] text-ink-3">
              LE 的 LEProc.exe 需在「设置 → 启动」中配置路径
            </p>
          </div>
        </div>

        <div>
          <label class="label">存档目录（留空则自动识别）</label>
          <input v-model="form.savePath" class="field" placeholder="可选，手动指定存档位置" />
        </div>

        <div>
          <label class="label">简介</label>
          <textarea v-model="form.description" class="field" rows="3" placeholder="可选" />
        </div>

        <div>
          <label class="label">备注</label>
          <textarea v-model="form.notes" class="field" rows="2" placeholder="可选" />
        </div>
      </div>
    </div>

    <template #footer>
      <button class="btn btn-ghost" @click="emit('close')">取消</button>
      <button class="btn btn-primary" :disabled="saving" @click="submit">
        {{ saving ? "保存中…" : isEdit ? "保存修改" : "添加到游戏库" }}
      </button>
    </template>
  </BaseModal>
</template>
