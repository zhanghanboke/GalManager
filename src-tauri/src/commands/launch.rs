//! 启动、停止与运行态查询命令。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::launcher::{self, LauncherState, LaunchOutcome, RunningGame};
use crate::paths::AppPaths;
use tauri::{AppHandle, State};

/// 启动游戏
#[tauri::command]
pub fn launch_game(
    app: AppHandle,
    db: State<'_, Db>,
    launcher: State<'_, LauncherState>,
    game_id: i64,
) -> AppResult<LaunchOutcome> {
    let game = db.get_game(game_id)?;
    launcher::launch(&app, &db, &launcher, &game)
}

/// 停止游戏
#[tauri::command]
pub fn stop_game(launcher: State<'_, LauncherState>, game_id: i64) -> AppResult<u32> {
    launcher::stop(&launcher, game_id)
}

/// 当前正在运行的游戏列表
#[tauri::command]
pub fn running_games(launcher: State<'_, LauncherState>) -> Vec<RunningGame> {
    launcher.running()
}

/// 打开游戏目录 / 存档目录
#[tauri::command]
pub fn open_path(path: String) -> AppResult<()> {
    launcher::open_in_explorer(std::path::Path::new(&path))
}

/// 在资源管理器中定位游戏目录
#[tauri::command]
pub fn open_game_folder(db: State<'_, Db>, game_id: i64) -> AppResult<()> {
    let game = db.get_game(game_id)?;
    let path = game
        .path
        .ok_or_else(|| AppError::msg("该游戏未设置安装目录"))?;
    launcher::open_in_explorer(std::path::Path::new(&path))
}

/// 打开应用数据目录（设置页「打开数据目录」）
#[tauri::command]
pub fn open_app_data_dir(paths: State<'_, AppPaths>) -> AppResult<()> {
    launcher::open_in_explorer(&paths.root)
}

/// 打开存档备份目录
#[tauri::command]
pub fn open_backup_dir(db: State<'_, Db>, paths: State<'_, AppPaths>) -> AppResult<()> {
    let configured = db.get_setting("save_backup_root")?.filter(|p| !p.trim().is_empty());
    let dir = configured
        .map(|p| std::path::PathBuf::from(crate::savedata::expand_env(&p)))
        .unwrap_or_else(|| paths.default_saves_dir());
    std::fs::create_dir_all(&dir)?;
    launcher::open_in_explorer(&dir)
}

/// 自动探测 Locale Emulator 安装路径
#[tauri::command]
pub fn detect_locale_emulator() -> Option<String> {
    launcher::detect_locale_emulator()
}
