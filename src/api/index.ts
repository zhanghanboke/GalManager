/**
 * 后端命令的类型化封装。
 *
 * 约定：组件与 store 只能通过本模块访问后端，禁止直接 `invoke`。
 * 这样后端命令重命名时只需改一处。
 */

import { invoke } from "@tauri-apps/api/core";

// ==================== 类型 ====================

export type PlayStatus = "unplayed" | "playing" | "completed" | "on_hold" | "dropped";

export interface Tag {
  id: number;
  name: string;
  color: string | null;
  gameCount: number;
}

export interface Category {
  id: number;
  name: string;
  icon: string | null;
  sortOrder: number;
  gameCount: number;
}

export interface Game {
  id: number;
  title: string;
  originalTitle: string | null;
  path: string | null;
  executable: string | null;
  args: string | null;
  coverPath: string | null;
  engine: string | null;
  engineConfidence: number;
  categoryId: number | null;
  playStatus: PlayStatus;
  favorite: number;
  rating: number;
  leLaunch: number;
  leLocale: string | null;
  totalPlaySeconds: number;
  lastPlayedAt: string | null;
  releaseDate: string | null;
  developer: string | null;
  description: string | null;
  notes: string | null;
  savePath: string | null;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
  tags: Tag[];
  sessionCount: number;
  saveCount: number;
}

export interface GameInput {
  title: string;
  originalTitle?: string | null;
  path?: string | null;
  executable?: string | null;
  args?: string | null;
  coverPath?: string | null;
  engine?: string | null;
  engineConfidence?: number | null;
  categoryId?: number | null;
  playStatus?: PlayStatus | null;
  favorite?: number | null;
  rating?: number | null;
  leLaunch?: number | null;
  leLocale?: string | null;
  releaseDate?: string | null;
  developer?: string | null;
  description?: string | null;
  notes?: string | null;
  savePath?: string | null;
  tags?: string[] | null;
}

export interface GameFilter {
  keyword?: string;
  categoryId?: number | null;
  tags?: string[];
  statuses?: string[];
  favoriteOnly?: boolean;
  engines?: string[];
  sortBy?: string;
  sortDesc?: boolean;
}

export interface ScanCandidate {
  name: string;
  path: string;
  executables: string[];
  engine: string | null;
  engineConfidence: number;
  alreadyImported: boolean;
}

export interface ScanOptions {
  root: string;
  maxDepth: number;
  mode: "executable" | "first_level";
  detectExecutables: boolean;
  detectEngine: boolean;
}

export interface EngineInfo {
  id: string;
  label: string;
  confidence: number;
  evidence: string[];
}

export interface EngineDescriptor {
  id: string;
  label: string;
  saveHint: string;
}

export interface LaunchOutcome {
  ok: boolean;
  message: string;
  pid: number | null;
  viaLocaleEmulator: boolean;
  tracking: boolean;
}

export interface RunningGame {
  gameId: number;
  title: string;
  processName: string;
  pid: number;
  startedAt: string;
  viaLocaleEmulator: boolean;
}

export interface SavePathCandidate {
  path: string;
  exists: boolean;
  fileCount: number;
  sizeBytes: number;
  source: string;
}

export interface SavePathProbe {
  engine: string;
  engineLabel: string;
  confidence: number;
  paths: SavePathCandidate[];
}

export interface SaveSlot {
  id: number;
  gameId: number;
  slotName: string;
  engine: string | null;
  sourcePath: string;
  backupPath: string;
  sizeBytes: number;
  fileCount: number;
  remark: string | null;
  createdAt: string;
}

export interface RestoreOutcome {
  restoredFiles: number;
  safetyBackup: string | null;
  targetPath: string;
}

export interface PlaySession {
  id: number;
  gameId: number;
  startedAt: string;
  endedAt: string | null;
  durationSeconds: number;
  note: string | null;
  gameTitle: string | null;
}

export interface Patch {
  id: number;
  gameId: number;
  name: string;
  version: string | null;
  patchType: string;
  filePath: string | null;
  url: string | null;
  installed: number;
  remark: string | null;
  createdAt: string;
}

export interface Note {
  id: number;
  gameId: number;
  title: string;
  content: string;
  updatedAt: string;
  createdAt: string;
}

export interface ResourceLink {
  id: number;
  gameId: number | null;
  title: string;
  url: string;
  kind: string;
  remark: string | null;
  createdAt: string;
}

export interface StatsOverview {
  totalGames: number;
  totalPlaySeconds: number;
  completedGames: number;
  playingGames: number;
  favoriteGames: number;
  weekPlaySeconds: number;
  monthPlaySeconds: number;
  saveSlotCount: number;
}

export interface DailyPlaytime {
  date: string;
  seconds: number;
  sessions: number;
}

export interface GamePlaytimeRank {
  gameId: number;
  title: string;
  coverPath: string | null;
  seconds: number;
  sessions: number;
}

export interface YearlyReport {
  year: number;
  totalSeconds: number;
  totalSessions: number;
  activeDays: number;
  averageSeconds: number;
  longestSessionSeconds: number;
  longestSessionGame: string | null;
  topGame: string | null;
  completedCount: number;
  addedCount: number;
  monthly: number[];
  weekday: number[];
  daily: DailyPlaytime[];
  ranking: GamePlaytimeRank[];
}

export interface StorageInfo {
  dataDir: string;
  dbSize: number;
  dbSizeHuman: string;
  coversSize: number;
  coversSizeHuman: string;
  backupsSize: number;
  backupsSizeHuman: string;
  backupRoot: string;
}

export interface AppInfo {
  name: string;
  version: string;
  tauriVersion: string;
  os: string;
  arch: string;
  dataDir: string;
}

export type SettingsMap = Record<string, string>;

// ==================== 游戏库 ====================

export const libraryApi = {
  list: (filter: GameFilter) => invoke<Game[]>("list_games", { filter }),
  get: (id: number) => invoke<Game>("get_game", { id }),
  create: (input: GameInput) => invoke<number>("create_game", { input }),
  update: (id: number, input: GameInput) => invoke<void>("update_game", { id, input }),
  remove: (id: number) => invoke<void>("delete_game", { id }),
  importMany: (inputs: GameInput[]) => invoke<number[]>("import_games", { inputs }),

  batchCategory: (ids: number[], categoryId: number | null) =>
    invoke<void>("batch_set_category", { ids, categoryId }),
  batchStatus: (ids: number[], status: PlayStatus) =>
    invoke<void>("batch_set_status", { ids, status }),
  batchFavorite: (ids: number[], favorite: boolean) =>
    invoke<void>("batch_set_favorite", { ids, favorite }),
  batchTags: (ids: number[], tags: string[]) => invoke<void>("batch_add_tags", { ids, tags }),
  reorder: (ids: number[]) => invoke<void>("reorder_games", { ids }),

  setCover: (gameId: number, sourcePath: string) =>
    invoke<string>("set_game_cover", { gameId, sourcePath }),
  clearCover: (gameId: number) => invoke<void>("clear_game_cover", { gameId }),

  setTags: (gameId: number, tags: string[]) => invoke<void>("set_game_tags", { gameId, tags }),
};

export const categoryApi = {
  list: () => invoke<Category[]>("list_categories"),
  create: (name: string, icon?: string) => invoke<number>("create_category", { name, icon }),
  update: (id: number, name: string, icon?: string) =>
    invoke<void>("update_category", { id, name, icon }),
  remove: (id: number) => invoke<void>("delete_category", { id }),
};

export const tagApi = {
  list: () => invoke<Tag[]>("list_tags"),
  update: (id: number, name: string, color?: string) =>
    invoke<void>("update_tag", { id, name, color }),
  remove: (id: number) => invoke<void>("delete_tag", { id }),
};

// ==================== 扫描 ====================

export const scanApi = {
  scan: (options: ScanOptions) => invoke<ScanCandidate[]>("scan_directory", { options }),
  detectEngine: (path: string) => invoke<EngineInfo | null>("detect_engine", { path }),
  listEngines: () => invoke<EngineDescriptor[]>("list_engines"),
  listExecutables: (path: string) => invoke<string[]>("list_executables", { path }),
  buildInputs: (candidates: ScanCandidate[]) => invoke<GameInput[]>("build_game_inputs", { candidates }),
};

// ==================== 启动 ====================

export const launchApi = {
  launch: (gameId: number) => invoke<LaunchOutcome>("launch_game", { gameId }),
  stop: (gameId: number) => invoke<number>("stop_game", { gameId }),
  running: () => invoke<RunningGame[]>("running_games"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  openGameFolder: (gameId: number) => invoke<void>("open_game_folder", { gameId }),
  openAppDataDir: () => invoke<void>("open_app_data_dir"),
  openBackupDir: () => invoke<void>("open_backup_dir"),
  detectLocaleEmulator: () => invoke<string | null>("detect_locale_emulator"),
};

// ==================== 存档 ====================

export const saveApi = {
  probe: (gameId: number) => invoke<SavePathProbe>("probe_save_paths", { gameId }),
  backup: (
    gameId: number,
    sourcePath: string,
    slotName?: string,
    remark?: string,
  ) => invoke<SaveSlot>("backup_save", { gameId, sourcePath, slotName, remark }),
  list: (gameId: number) => invoke<SaveSlot[]>("list_save_slots", { gameId }),
  restore: (slotId: number, targetPath?: string, backupExisting?: boolean) =>
    invoke<RestoreOutcome>("restore_save", { slotId, targetPath, backupExisting }),
  remove: (slotId: number, deleteFile?: boolean) =>
    invoke<void>("delete_save_slot", { slotId, deleteFile }),
  setRemark: (slotId: number, remark: string | null) =>
    invoke<void>("update_save_remark", { slotId, remark }),
  inspect: (slotId: number) => invoke<[string, number][]>("inspect_save_archive", { slotId }),
  setSavePath: (gameId: number, savePath: string | null) =>
    invoke<void>("set_game_save_path", { gameId, savePath }),
  backupAll: () => invoke<[number, string, boolean][]>("backup_all_saves"),
};

// ==================== 统计 ====================

export const statsApi = {
  overview: () => invoke<StatsOverview>("stats_overview"),
  daily: (days?: number) => invoke<DailyPlaytime[]>("stats_daily", { days }),
  ranking: (limit?: number) => invoke<GamePlaytimeRank[]>("stats_ranking", { limit }),
  yearly: (year: number) => invoke<YearlyReport>("stats_yearly_report", { year }),
  playYears: () => invoke<number[]>("stats_play_years"),
  engineDistribution: () => invoke<[string, number][]>("stats_engine_distribution"),
  recentSessions: (limit?: number) => invoke<PlaySession[]>("stats_recent_sessions", { limit }),
  gameSessions: (gameId: number, limit?: number) =>
    invoke<PlaySession[]>("list_game_sessions", { gameId, limit }),
  deleteSession: (sessionId: number) => invoke<void>("delete_session", { sessionId }),
  addSession: (gameId: number, startedAt: string, durationMinutes: number) =>
    invoke<number>("add_session", { gameId, startedAt, durationMinutes }),
};

// ==================== 设置 ====================

export const settingsApi = {
  get: () => invoke<SettingsMap>("get_settings"),
  set: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
  setMany: (values: SettingsMap) => invoke<void>("set_settings", { values }),
  reset: () => invoke<SettingsMap>("reset_settings"),
  storage: () => invoke<StorageInfo>("storage_info"),
  clearCoverCache: () => invoke<number>("clear_cover_cache"),
  optimizeDb: () => invoke<string>("optimize_database"),
  appInfo: () => invoke<AppInfo>("app_info"),
};

// ==================== 辅助工具 ====================

export const extrasApi = {
  listPatches: (gameId: number) => invoke<Patch[]>("list_patches", { gameId }),
  savePatch: (payload: {
    id?: number | null;
    gameId: number;
    name: string;
    version?: string | null;
    patchType: string;
    filePath?: string | null;
    url?: string | null;
    installed: boolean;
    remark?: string | null;
  }) => invoke<number>("save_patch", payload),
  deletePatch: (id: number) => invoke<void>("delete_patch", { id }),
  togglePatch: (id: number, installed: boolean) =>
    invoke<void>("toggle_patch_installed", { id, installed }),

  listNotes: (gameId: number) => invoke<Note[]>("list_notes", { gameId }),
  saveNote: (payload: { id?: number | null; gameId: number; title: string; content: string }) =>
    invoke<number>("save_note", payload),
  deleteNote: (id: number) => invoke<void>("delete_note", { id }),

  listLinks: (gameId?: number | null) => invoke<ResourceLink[]>("list_links", { gameId }),
  addLink: (payload: {
    gameId?: number | null;
    title: string;
    url: string;
    kind: string;
    remark?: string | null;
  }) => invoke<number>("add_link", payload),
  deleteLink: (id: number) => invoke<void>("delete_link", { id }),
};
