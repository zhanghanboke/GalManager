//! 存档备份的共享逻辑。
//!
//! 「探测最佳存档目录 → 打包成 zip → 落库」这套流程，命令层（手动备份）与
//! 后台自动备份都要用。放在这里统一实现，避免两处各写一遍导致行为漂移
//! （例如手动指定过存档目录的游戏，批量备份时却没走手动路径）。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::{Game, SavePathCandidate, SavePathProbe, SaveSlot};
use crate::paths::{sanitize_file_name, AppPaths};
use crate::savedata;
use std::path::PathBuf;

/// 备份根目录：优先用户设置，否则用应用数据目录
pub fn resolve_backup_root(db: &Db, paths: &AppPaths) -> AppResult<PathBuf> {
    let configured = db
        .get_setting("save_backup_root")?
        .filter(|p| !p.trim().is_empty());
    match configured {
        // 允许用户填写 `%APPDATA%\GalManager\saves` 这类带环境变量的路径
        Some(dir) => crate::paths::ensure_dir(&savedata::expand_env(&dir)),
        None => Ok(paths.default_saves_dir()),
    }
}

/// 构造「用户手动指定」的探测结果
fn manual_probe(game: &Game, raw_path: &str) -> SavePathProbe {
    let expanded = savedata::expand_env(raw_path);
    let path = PathBuf::from(&expanded);
    let exists = path.is_dir();
    let (file_count, size_bytes) = if exists {
        savedata::measure_dir(&path)
    } else {
        (0, 0)
    };
    SavePathProbe {
        engine: game.engine.clone().unwrap_or_else(|| "other".into()),
        engine_label: crate::engine::label_of(game.engine.as_deref().unwrap_or("other")),
        confidence: 100,
        paths: vec![SavePathCandidate {
            path: expanded,
            exists,
            file_count,
            size_bytes,
            source: "手动指定".to_string(),
        }],
    }
}

/// 探测某游戏的存档目录候选；用户手动指定过则优先使用它
pub fn probe_game(db: &Db, game_id: i64) -> AppResult<SavePathProbe> {
    let game = db.get_game(game_id)?;
    if let Some(manual) = game.save_path.as_ref().filter(|p| !p.trim().is_empty()) {
        return Ok(manual_probe(&game, manual));
    }
    savedata::probe(
        game.path.as_deref(),
        game.engine.as_deref(),
        &game.title,
        game.executable.as_deref(),
    )
}

/// 从探测结果里挑最合适的存档目录：优先「存在且非空」
pub fn best_candidate(probe: &SavePathProbe) -> Option<&SavePathCandidate> {
    probe
        .paths
        .iter()
        .find(|c| c.exists && c.file_count > 0)
        .or_else(|| probe.paths.iter().find(|c| c.exists))
}

/// 备份单个存档目录并写入槽位记录。
///
/// `slot_name` 为空时回退为「自动备份」。
pub fn backup_one(
    db: &Db,
    paths: &AppPaths,
    game_id: i64,
    source_path: &str,
    slot_name: &str,
    remark: Option<&str>,
) -> AppResult<SaveSlot> {
    let game = db.get_game(game_id)?;
    let source = PathBuf::from(source_path);
    if !source.is_dir() {
        return Err(AppError::msg(format!(
            "存档目录不存在: {source_path}，请先在详情页确认存档位置"
        )));
    }

    let backup_root = resolve_backup_root(db, paths)?;
    let game_dir = backup_root.join(format!("{}_{}", game_id, sanitize_file_name(&game.title)));
    std::fs::create_dir_all(&game_dir)?;

    // 时间戳带毫秒：同一次会话里连续备份两次（手动 + 自动，或测试）也不会撞名。
    // 只精确到秒时，第二次会直接覆盖第一次的归档文件，而数据库里却留下两条记录。
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S%3f").to_string();
    let label = sanitize_file_name(slot_name);
    let label = if label.is_empty() {
        "自动备份".to_string()
    } else {
        label
    };

    // 极端情况下（同毫秒）再加序号兜底，确保绝不覆盖已有归档
    let mut archive = game_dir.join(format!("{}_{}.zip", label, stamp));
    let mut suffix = 1;
    while archive.exists() {
        suffix += 1;
        archive = game_dir.join(format!("{}_{}_{}.zip", label, stamp, suffix));
    }

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
        &label,
        game.engine.as_deref(),
        &source.to_string_lossy(),
        &outcome.backup_path,
        outcome.size_bytes,
        outcome.file_count,
        remark,
    )?;

    db.get_save_slot(slot_id)
}
