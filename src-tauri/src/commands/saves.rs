//! 存档探测、备份、还原命令。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::{SavePathProbe, SaveSlot};
use crate::paths::{sanitize_file_name, AppPaths};
use crate::savedata::{self, BackupOutcome, RestoreOptions, RestoreOutcome};
use std::path::PathBuf;
use tauri::State;

/// 探测某游戏的存档目录候选
#[tauri::command]
pub fn probe_save_paths(db: State<'_, Db>, game_id: i64) -> AppResult<SavePathProbe> {
    let game = db.get_game(game_id)?;
    // 用户手动指定过存档目录时，优先返回它
    // 支持写 `%APPDATA%\XXX` 这类带环境变量的路径
    if let Some(manual) = game.save_path.as_ref().filter(|p| !p.trim().is_empty()) {
        let expanded = savedata::expand_env(manual);
        let path = PathBuf::from(&expanded);
        let exists = path.is_dir();
        let (file_count, size_bytes) = if exists {
            (
                walkdir::WalkDir::new(&path)
                    .max_depth(8)
                    .into_iter()
                    .flatten()
                    .filter(|e| e.file_type().is_file())
                    .count() as i64,
                crate::paths::path_size(&path),
            )
        } else {
            (0, 0)
        };
        return Ok(SavePathProbe {
            engine: game.engine.clone().unwrap_or_else(|| "other".into()),
            engine_label: crate::engine::label_of(game.engine.as_deref().unwrap_or("other")),
            confidence: 100,
            paths: vec![crate::models::SavePathCandidate {
                path: expanded.clone(),
                exists,
                file_count,
                size_bytes,
                source: "手动指定".to_string(),
            }],
        });
    }

    savedata::probe(
        game.path.as_deref(),
        game.engine.as_deref(),
        &game.title,
        game.executable.as_deref(),
    )
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
    let game = db.get_game(game_id)?;
    let source = PathBuf::from(&source_path);
    if !source.is_dir() {
        return Err(AppError::msg(format!(
            "存档目录不存在: {source_path}，请先在详情页确认存档位置"
        )));
    }

    let backup_root = resolve_backup_root(&db, &paths)?;
    let game_dir = backup_root.join(format!(
        "{}_{}",
        game_id,
        sanitize_file_name(&game.title)
    ));
    std::fs::create_dir_all(&game_dir)?;

    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let label = slot_name
        .as_deref()
        .map(|s| sanitize_file_name(s))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "自动备份".to_string());
    let archive = game_dir.join(format!("{}_{}.zip", label, stamp));

    let outcome: BackupOutcome = savedata::create_backup(
        game_id,
        &game.title,
        game.engine.as_deref(),
        &source,
        &archive,
        remark.as_deref(),
    )?;

    let slot_id = db.insert_save_slot(
        game_id,
        &label,
        game.engine.as_deref(),
        &source.to_string_lossy(),
        &outcome.backup_path,
        outcome.size_bytes,
        outcome.file_count,
        remark.as_deref(),
    )?;

    db.get_save_slot(slot_id)
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
        let probe = match savedata::probe(
            game.path.as_deref(),
            game.engine.as_deref(),
            &game.title,
            game.executable.as_deref(),
        ) {
            Ok(p) => p,
            Err(error) => {
                report.push((game.id, format!("探测失败: {error}"), false));
                continue;
            }
        };

        let best = probe
            .paths
            .iter()
            .find(|c| c.exists && c.file_count > 0)
            .or_else(|| probe.paths.iter().find(|c| c.exists));

        let Some(candidate) = best else {
            report.push((game.id, "未找到存档目录，已跳过".to_string(), false));
            continue;
        };

        match backup_one(&db, &paths, game.id, &candidate.path, None) {
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

/// 内部复用：备份单个目录
fn backup_one(
    db: &Db,
    paths: &AppPaths,
    game_id: i64,
    source_path: &str,
    remark: Option<&str>,
) -> AppResult<SaveSlot> {
    let game = db.get_game(game_id)?;
    let source = PathBuf::from(source_path);
    let backup_root = resolve_backup_root(db, paths)?;
    let game_dir = backup_root.join(format!("{}_{}", game_id, sanitize_file_name(&game.title)));
    std::fs::create_dir_all(&game_dir)?;

    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let archive = game_dir.join(format!("自动备份_{}.zip", stamp));

    let outcome = savedata::create_backup(
        game_id,
        &game.title,
        game.engine.as_deref(),
        &source,
        &archive,
        remark,
    )?;

    let slot_id = db.insert_save_slot(
        game_id,
        "自动备份",
        game.engine.as_deref(),
        source_path,
        &outcome.backup_path,
        outcome.size_bytes,
        outcome.file_count,
        remark,
    )?;
    db.get_save_slot(slot_id)
}

/// 备份根目录：优先用户设置，否则用应用数据目录
fn resolve_backup_root(db: &Db, paths: &AppPaths) -> AppResult<PathBuf> {
    let configured = db
        .get_setting("save_backup_root")?
        .filter(|p| !p.trim().is_empty());
    match configured {
        // 允许用户填写 `%APPDATA%\GalManager\saves` 这类带环境变量的路径
        Some(dir) => crate::paths::ensure_dir(&crate::savedata::expand_env(&dir)),
        None => Ok(paths.default_saves_dir()),
    }
}
