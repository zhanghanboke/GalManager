//! 系统托盘。
//!
//! 提供「显示主界面 / 退出」两个入口；关闭窗口时默认最小化到托盘而非退出，
//! 避免用户误关导致正在统计的游玩时长中断。

use crate::db::Db;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

/// 显示并聚焦主窗口
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 判断是否启用「关闭时最小化到托盘」
pub fn close_to_tray_enabled(db: &Db) -> bool {
    db.get_setting("close_to_tray")
        .ok()
        .flatten()
        .map(|value| value != "false")
        .unwrap_or(true)
}

pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "显示主界面", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 GalManager", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id("galmanager-tray")
        .tooltip("GalManager · Galgame 管理器")
        .menu(&menu)
        // 左键点击托盘图标时只显示窗口，不弹出菜单
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => {
                log::info!("用户从托盘菜单退出应用");
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    log::info!("系统托盘已就绪");
    Ok(())
}
