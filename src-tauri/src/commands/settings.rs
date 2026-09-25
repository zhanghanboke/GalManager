//! 应用设置命令。
//!
//! 设置项以 key-value 形式存放于 SQLite，避免引入额外的配置文件格式。

use crate::db::Db;
use crate::error::AppResult;
use crate::paths::{human_size, path_size, AppPaths};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

/// 设置项的默认值表。前端首次启动时用它渲染表单。
pub fn defaults() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("le_path".into(), String::new());
    map.insert("le_args_template".into(), crate::launcher::DEFAULT_LE_TEMPLATE.into());
    map.insert("default_le_locale".into(), "ja-JP".into());
    map.insert("save_backup_root".into(), String::new());
    map.insert("scan_roots".into(), "[]".into());
    map.insert("scan_max_depth".into(), "3".into());
    map.insert("grid_size".into(), "md".into());
    map.insert("view_mode".into(), "grid".into());
    map.insert("sort_by".into(), "title".into());
    map.insert("sort_desc".into(), "false".into());
    map.insert("minimize_to_tray".into(), "true".into());
    map.insert("close_to_tray".into(), "true".into());
    map.insert("auto_scan_on_start".into(), "false".into());
    map.insert("auto_backup_saves".into(), "false".into());
    map.insert("show_uncategorized".into(), "true".into());
    map
}

#[tauri::command]
pub fn get_settings(db: State<'_, Db>) -> AppResult<HashMap<String, String>> {
    let stored = db.all_settings()?;
    let mut merged = defaults();
    for (key, value) in stored {
        merged.insert(key, value);
    }
    Ok(merged)
}

#[tauri::command]
pub fn set_setting(db: State<'_, Db>, key: String, value: String) -> AppResult<()> {
    db.set_setting(&key, &value)
}

#[tauri::command]
pub fn set_settings(db: State<'_, Db>, values: HashMap<String, String>) -> AppResult<()> {
    for (key, value) in values {
        db.set_setting(&key, &value)?;
    }
    Ok(())
}

#[tauri::command]
pub fn reset_settings(db: State<'_, Db>) -> AppResult<HashMap<String, String>> {
    for key in defaults().keys() {
        db.set_setting(key, "")?;
    }
    // 清空后重新返回「默认 + 已存」合并结果
    get_settings(db)
}

/// 数据库与数据目录体积信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub data_dir: String,
    pub db_size: i64,
    pub db_size_human: String,
    pub covers_size: i64,
    pub covers_size_human: String,
    pub backups_size: i64,
    pub backups_size_human: String,
    pub backup_root: String,
}

#[tauri::command]
pub fn storage_info(db: State<'_, Db>, paths: State<'_, AppPaths>) -> AppResult<StorageInfo> {
    let configured_backup = db
        .get_setting("save_backup_root")?
        .filter(|p| !p.trim().is_empty());
    let backup_root = configured_backup
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| paths.default_saves_dir());

    let db_size = path_size(&paths.db_file());
    let covers_size = path_size(&paths.covers_dir());
    let backups_size = path_size(&backup_root);

    Ok(StorageInfo {
        data_dir: paths.root.to_string_lossy().to_string(),
        db_size,
        db_size_human: human_size(db_size),
        covers_size,
        covers_size_human: human_size(covers_size),
        backups_size,
        backups_size_human: human_size(backups_size),
        backup_root: backup_root.to_string_lossy().to_string(),
    })
}

/// 清空封面缓存目录
#[tauri::command]
pub fn clear_cover_cache(paths: State<'_, AppPaths>) -> AppResult<i64> {
    let dir = paths.covers_dir();
    let mut removed = 0i64;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                if std::fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    log::info!("已清理封面缓存 {} 个文件", removed);
    Ok(removed)
}

/// 数据库维护：VACUUM + WAL 检查点
#[tauri::command]
pub fn optimize_database(db: State<'_, Db>) -> AppResult<String> {
    db.with_conn(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;
        Ok(())
    })?;
    Ok("数据库已整理完成".to_string())
}

/// 应用信息（关于页）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub tauri_version: String,
    pub os: String,
    pub arch: String,
    pub data_dir: String,
}

#[tauri::command]
pub fn app_info(paths: State<'_, AppPaths>) -> AppInfo {
    AppInfo {
        name: "GalManager".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: tauri::VERSION.to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        data_dir: paths.root.to_string_lossy().to_string(),
    }
}
