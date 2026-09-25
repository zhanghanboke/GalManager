/**
 * 拖拽导入的跨组件通道。
 *
 * `App.vue` 监听系统级文件拖放（Tauri `onDragDropEvent`），拿到路径后写入
 * `pendingDropPaths`；`LibraryView` 消费它并带着路径打开扫描对话框。
 *
 * 之所以不直接在 `App.vue` 里放扫描对话框：扫描对话框属于游戏库页面，
 * 放在全局会导致同一时间存在两个实例。
 */
import { ref } from "vue";

/** 待处理的拖入路径（消费后清空） */
export const pendingDropPaths = ref<string[]>([]);

/** 是否正有文件悬停在窗口上（用于显示高亮遮罩） */
export const dragActive = ref(false);

/** 提交一批拖入的路径 */
export function submitDroppedPaths(paths: string[]) {
  const cleaned = paths.map((p) => p.trim()).filter(Boolean);
  if (!cleaned.length) return false;
  pendingDropPaths.value = cleaned;
  return true;
}
