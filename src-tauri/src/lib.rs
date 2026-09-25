//! GalManager 后端入口。
//!
//! 模块划分：
//! - `db`        数据库连接与全部查询
//! - `models`    前后端共享的数据结构
//! - `scanner`   目录扫描与可执行文件识别
//! - `engine`    Galgame 引擎识别
//! - `savedata`  存档路径探测与备份 / 还原
//! - `launcher`  游戏启动（含 Locale Emulator 转区）与时长监控
//! - `commands`  Tauri 命令层
//! - `tray`      系统托盘

mod autobackup;
mod commands;
mod db;
mod engine;
mod error;
mod launcher;
mod library;
mod models;
mod paths;
mod save_backup;
mod savedata;
mod scanner;
mod search;
mod tray;

use db::Db;
use launcher::LauncherState;
use paths::AppPaths;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例：第二次启动时聚焦已有窗口，避免多开导致数据库写入冲突
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("galmanager".into()),
                    }),
                ])
                .build(),
        )
        .setup(|app| {
            // ---- 应用数据目录 ----
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| format!("无法定位应用数据目录: {error}"))?;
            let paths = AppPaths::new(data_dir)?;
            log::info!("应用数据目录: {}", paths.root.display());

            // ---- 数据库 ----
            let db = Db::open(&paths.db_file())?;
            log::info!("数据库就绪: {}", paths.db_file().display());

            app.manage(paths);
            app.manage(db);
            app.manage(LauncherState::new());

            // ---- 系统托盘 ----
            if let Err(error) = tray::setup_tray(app.handle()) {
                log::warn!("托盘初始化失败（不影响主功能）: {error}");
            }

            // ---- 存档自动备份调度（后台线程，仅在设置开启时真正执行）----
            autobackup::spawn(app.handle().clone());

            // 首次启动时自动探测 Locale Emulator，省去用户手动配置
            let db_state = app.state::<Db>();
            match db_state.get_setting("le_path") {
                Ok(None) => {
                    if let Some(path) = launcher::detect_locale_emulator() {
                        let _ = db_state.set_setting("le_path", &path);
                        log::info!("已自动配置 Locale Emulator: {path}");
                    }
                }
                Ok(Some(existing)) if existing.trim().is_empty() => {
                    if let Some(path) = launcher::detect_locale_emulator() {
                        let _ = db_state.set_setting("le_path", &path);
                    }
                }
                _ => {}
            }

            // 窗口在配置中设为 visible: false（避免 WebView 初始化期间的白屏闪烁），
            // 初始化完成后在这里显式显示。
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let db = app.state::<Db>();
                if tray::close_to_tray_enabled(&db) {
                    // 拦截关闭：隐藏到托盘，保证游玩计时不被中断
                    api.prevent_close();
                    let _ = window.hide();
                    log::info!("窗口已最小化到托盘");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // ---- 游戏库 ----
            commands::games::list_games,
            commands::games::get_game,
            commands::games::create_game,
            commands::games::update_game,
            commands::games::delete_game,
            commands::games::import_games,
            commands::games::batch_set_category,
            commands::games::batch_set_status,
            commands::games::batch_set_favorite,
            commands::games::batch_add_tags,
            commands::games::reorder_games,
            commands::games::set_game_cover,
            commands::games::clear_game_cover,
            commands::games::suggest_cover_name,
            // ---- 分类 / 标签 ----
            commands::games::list_categories,
            commands::games::create_category,
            commands::games::update_category,
            commands::games::delete_category,
            commands::games::list_tags,
            commands::games::update_tag,
            commands::games::delete_tag,
            commands::games::set_game_tags,
            // ---- 扫描 ----
            commands::scan::scan_directory,
            commands::scan::detect_engine,
            commands::scan::list_engines,
            commands::scan::list_executables,
            commands::scan::build_game_inputs,
            // ---- 启动 ----
            commands::launch::launch_game,
            commands::launch::stop_game,
            commands::launch::running_games,
            commands::launch::open_path,
            commands::launch::open_game_folder,
            commands::launch::open_app_data_dir,
            commands::launch::open_backup_dir,
            commands::launch::detect_locale_emulator,
            // ---- 存档 ----
            commands::saves::probe_save_paths,
            commands::saves::backup_save,
            commands::saves::list_save_slots,
            commands::saves::restore_save,
            commands::saves::delete_save_slot,
            commands::saves::update_save_remark,
            commands::saves::inspect_save_archive,
            commands::saves::read_save_manifest,
            commands::saves::set_game_save_path,
            commands::saves::backup_all_saves,
            commands::saves::auto_backup_status,
            commands::saves::run_auto_backup_now,
            // ---- 统计 ----
            commands::stats::stats_overview,
            commands::stats::stats_daily,
            commands::stats::stats_ranking,
            commands::stats::stats_yearly_report,
            commands::stats::stats_play_years,
            commands::stats::stats_engine_distribution,
            commands::stats::stats_recent_sessions,
            commands::stats::list_game_sessions,
            commands::stats::delete_session,
            commands::stats::add_session,
            // ---- 导入 / 导出 ----
            commands::transfer::export_library,
            commands::transfer::inspect_library_archive,
            commands::transfer::import_library,
            // ---- 设置 ----
            commands::settings::get_settings,
            commands::settings::set_setting,
            commands::settings::set_settings,
            commands::settings::reset_settings,
            commands::settings::storage_info,
            commands::settings::clear_cover_cache,
            commands::settings::optimize_database,
            commands::settings::app_info,
            // ---- 辅助工具 ----
            commands::extras::list_patches,
            commands::extras::save_patch,
            commands::extras::delete_patch,
            commands::extras::toggle_patch_installed,
            commands::extras::list_notes,
            commands::extras::save_note,
            commands::extras::delete_note,
            commands::extras::list_links,
            commands::extras::add_link,
            commands::extras::delete_link,
        ])
        .run(tauri::generate_context!())
        .expect("GalManager 启动失败");
}
