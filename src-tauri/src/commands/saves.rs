//! 存档探测、备份、还原命令。
//!
//! 探测与备份的实际逻辑在 `save_backup` 模块，这里只做参数适配，
//! 以便后台自动备份复用同一套行为。

use crate::db::Db;
use crate::error::AppResult;
use crate::models::{SavePathProbe, SaveSlot};
use crate::paths::AppPaths;
use crate::save_backup;
use crate::savedata::{self, RestoreOptions, RestoreOutcome};
use std::path::PathBuf;
use tauri::State;

/// 探测某游戏的存档目录候选
#[tauri::command]
pub fn probe_save_paths(db: State<'_, Db>, game_id: i64) -> AppResult<SavePathProbe> {
    save_backup::probe_game(&db, game_id)
}

/// 备份存档
#[tauri::command]
pub fn backup_save(
    db: State<'_, Db>,
    paths: State<'_, AppPaths>,
    game_id: i64,
    source_path: String,
    slot_name: Option<String>,
    remark: Option<String>,
) -> AppResult<SaveSlot> {
    save_backup::backup_one(
        &db,
        &paths,
        game_id,
        &source_path,
        slot_name.as_deref().unwrap_or("手动备份"),
        remark.as_deref(),
    )
}

/// 列出某游戏的全部存档槽位
#[tauri::command]
pub fn list_save_slots(db: State<'_, Db>, game_id: i64) -> AppResult<Vec<SaveSlot>> {
    db.list_save_slots(game_id)
}

/// 还原存档
#[tauri::command]
pub fn restore_save(
    db: State<'_, Db>,
    slot_id: i64,
    target_path: Option<String>,
    backup_existing: Option<bool>,
) -> AppResult<RestoreOutcome> {
    let slot = db.get_save_slot(slot_id)?;
    let target = target_path
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| slot.source_path.clone());

    savedata::restore(&RestoreOptions {
        archive_path: slot.backup_path.clone(),
        target_path: target,
        backup_existing: backup_existing.unwrap_or(true),
    })
}

/// 删除存档槽位（可选同时删除归档文件）
#[tauri::command]
pub fn delete_save_slot(
    db: State<'_, Db>,
    slot_id: i64,
    delete_file: Option<bool>,
) -> AppResult<()> {
    let slot = db.get_save_slot(slot_id)?;
    if delete_file.unwrap_or(true) {
        let path = PathBuf::from(&slot.backup_path);
        if path.is_file() {
            if let Err(error) = std::fs::remove_file(&path) {
                log::warn!("删除归档失败 {}: {}", path.display(), error);
            }
        }
    }
    db.delete_save_slot(slot_id)
}

/// 修改槽位备注
#[tauri::command]
pub fn update_save_remark(db: State<'_, Db>, slot_id: i64, remark: Option<String>) -> AppResult<()> {
    db.update_save_slot_remark(slot_id, remark.as_deref())
}

/// 查看归档内容（还原前预览）
#[tauri::command]
pub fn inspect_save_archive(
    db: State<'_, Db>,
    slot_id: i64,
) -> AppResult<Vec<(String, i64)>> {
    let slot = db.get_save_slot(slot_id)?;
    savedata::inspect_archive(&PathBuf::from(&slot.backup_path))
}

/// 读取归档内的 manifest
#[tauri::command]
pub fn read_save_manifest(
    db: State<'_, Db>,
    slot_id: i64,
) -> AppResult<savedata::BackupManifest> {
    let slot = db.get_save_slot(slot_id)?;
    savedata::read_manifest(&PathBuf::from(&slot.backup_path))
}

/// 手动指定 / 清除游戏的存档目录
#[tauri::command]
pub fn set_game_save_path(
    db: State<'_, Db>,
    game_id: i64,
    save_path: Option<String>,
) -> AppResult<()> {
    let game = db.get_game(game_id)?;
    db.update_game(
        game_id,
        &crate::models::GameInput {
            title: game.title,
            save_path: Some(save_path.unwrap_or_default()),
            ..Default::default()
        },
    )
}

/// 一键备份全部可识别存档的游戏
#[tauri::command]
pub fn backup_all_saves(
    db: State<'_, Db>,
    paths: State<'_, AppPaths>,
) -> AppResult<Vec<(i64, String, bool)>> {
    let games = db.list_games(&crate::models::GameFilter::default())?;
    let mut report = Vec::new();

    for game in games {
        // 走 probe_game：用户手动指定过存档目录的游戏会用他指定的路径
        let probe = match save_backup::probe_game(&db, game.id) {
            Ok(probe) => probe,
            Err(error) => {
                report.push((game.id, format!("探测失败: {error}"), false));
                continue;
            }
        };

        let Some(candidate) = save_backup::best_candidate(&probe) else {
            report.push((game.id, "未找到存档目录，已跳过".to_string(), false));
            continue;
        };

        match save_backup::backup_one(
            &db,
            &paths,
            game.id,
            &candidate.path,
            "手动备份",
            None,
        ) {
            Ok(slot) => report.push((
                game.id,
                format!("已备份 {} 个文件", slot.file_count),
                true,
            )),
            Err(error) => report.push((game.id, format!("备份失败: {error}"), false)),
        }
    }

    Ok(report)
}

/// 自动备份状态（设置页展示）
#[tauri::command]
pub fn auto_backup_status(db: State<'_, Db>) -> AppResult<crate::autobackup::AutoBackupStatus> {
    crate::autobackup::status(&db)
}

/// 立即执行一次自动备份。
///
/// 不受间隔限制，但仍然只备份内容有变化的存档，
/// 因此重复点击不会产生一堆相同的归档。
#[tauri::command]
pub fn run_auto_backup_now(
    db: State<'_, Db>,
    paths: State<'_, AppPaths>,
) -> AppResult<crate::autobackup::AutoBackupReport> {
    crate::autobackup::run_once(&db, &paths)
}
