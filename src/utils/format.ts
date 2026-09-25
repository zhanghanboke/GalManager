/** 通用格式化与工具函数 */

/** 秒 → 「12 小时 34 分」 */
export function formatDuration(seconds: number): string {
  if (!seconds || seconds <= 0) return "0 分";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours === 0) {
    return minutes === 0 ? "不足 1 分" : `${minutes} 分`;
  }
  if (minutes === 0) return `${hours} 小时`;
  return `${hours} 小时 ${minutes} 分`;
}

/** 秒 → 「12.5 h」紧凑形式（用于排行榜） */
export function formatHours(seconds: number): string {
  const hours = seconds / 3600;
  if (hours < 1) return `${Math.round(seconds / 60)} min`;
  return `${hours.toFixed(1)} h`;
}

/** 字节 → 人类可读 */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return index === 0 ? `${bytes} B` : `${value.toFixed(1)} ${units[index]}`;
}

/** 相对时间：「3 天前」 */
export function formatRelative(input: string | null | undefined): string {
  if (!input) return "从未游玩";
  const date = parseLocal(input);
  if (!date) return input;
  const diff = Date.now() - date.getTime();
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} 天前`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months} 个月前`;
  return `${Math.floor(months / 12)} 年前`;
}

/** 解析后端返回的 "YYYY-MM-DD HH:MM:SS"（本地时间） */
export function parseLocal(input: string): Date | null {
  const match = input.match(
    /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2}))?)?/,
  );
  if (!match) return null;
  return new Date(
    Number(match[1]),
    Number(match[2]) - 1,
    Number(match[3]),
    Number(match[4] ?? 0),
    Number(match[5] ?? 0),
    Number(match[6] ?? 0),
  );
}

/** 只保留日期部分 */
export function formatDate(input: string | null | undefined): string {
  if (!input) return "—";
  const date = parseLocal(input);
  if (!date) return input;
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function formatDateTime(input: string | null | undefined): string {
  if (!input) return "—";
  const date = parseLocal(input);
  if (!date) return input;
  return `${formatDate(input)} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function pad(value: number): string {
  return value.toString().padStart(2, "0");
}

/** 取路径最后一段 */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** 游玩状态 → 中文标签 */
export const STATUS_LABELS: Record<string, string> = {
  unplayed: "未开始",
  playing: "在玩",
  completed: "已通关",
  on_hold: "搁置",
  dropped: "弃坑",
};

/** 游玩状态 → 颜色（Tailwind 任意值写法） */
export const STATUS_COLORS: Record<string, string> = {
  unplayed: "#6d7689",
  playing: "#4dd4d4",
  completed: "#7dd67d",
  on_hold: "#f0b429",
  dropped: "#f2555a",
};

/** 标签颜色盘 */
export const TAG_PALETTE = [
  "#8b7cf6",
  "#f472b6",
  "#4dd4d4",
  "#f0b429",
  "#7dd67d",
  "#f2555a",
  "#60a5fa",
  "#c084fc",
];

/** 根据字符串稳定地取一个颜色（同名标签颜色固定） */
export function colorOf(text: string): string {
  let hash = 0;
  for (let i = 0; i < text.length; i += 1) {
    hash = (hash * 31 + text.charCodeAt(i)) >>> 0;
  }
  return TAG_PALETTE[hash % TAG_PALETTE.length];
}

/** 简单的防抖 */
export function debounce<T extends (...args: never[]) => void>(fn: T, wait = 220) {
  let timer: ReturnType<typeof setTimeout> | null = null;
  return (...args: Parameters<T>) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => fn(...args), wait);
  };
}

/** 年份选项：从今年往前 10 年 */
export function yearOptions(): number[] {
  const current = new Date().getFullYear();
  return Array.from({ length: 10 }, (_, i) => current - i);
}
