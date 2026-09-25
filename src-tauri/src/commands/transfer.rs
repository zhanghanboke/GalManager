//! 游戏库导入 / 导出命令。
//!
//! 归档文件由前端通过系统对话框选定路径，后端只负责读写，
//! 与封面选择的处理方式保持一致。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::library::{self, ArchiveSummary, ExportOutcome, ImportMode, ImportOutcome, LibraryArchive};
use crate::paths::AppPaths;
use tauri::State;

/// 读取并解析归档文件（同时完成格式校验）
fn read_archive(source_path: &str) -> AppResult<LibraryArchive> {
    let path = source_path.trim();
    if path.is_empty() {
        return Err(AppError::msg("归档路径不能为空"));
    }
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Err(AppError::msg("归档文件是空的"));
    }
    let archive: LibraryArchive = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::msg(format!("解析归档失败：{error}")))?;
    archive.validate()?;
    Ok(archive)
}

/// 把主库文件复制一份留档。
///
/// 替换模式会清空现有库，属于破坏性操作，先落一份安全副本；
/// WAL 中可能仍有未落盘的事务，因此先做检查点再复制。
fn safety_copy(db: &Db, paths: &AppPaths) -> AppResult<String> {
    db.with_conn(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    })?;
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let target = paths
        .root
        .join(format!("galmanager.before_import_{stamp}.db"));
    std::fs::copy(paths.db_file(), &target)?;
    Ok(target.to_string_lossy().to_string())
}

/// 导出整库到指定 JSON 文件
#[tauri::command]
pub fn export_library(db: State<'_, Db>, target_path: String) -> AppResult<ExportOutcome> {
    let path = target_path.trim();
    if path.is_empty() {
        return Err(AppError::msg("导出路径不能为空"));
    }

    let archive = library::export(&db)?;
    let summary = archive.summary();
    let json = serde_json::to_vec_pretty(&archive)?;

    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, &json)?;

    log::info!("已导出游戏库（{} 个游戏）到 {}", summary.game_count, path);
    Ok(ExportOutcome {
        path: path.to_string(),
        bytes: json.len() as i64,
        summary,
    })
}

/// 只读取归档摘要，供导入前预览与确认
#[tauri::command]
pub fn inspect_library_archive(source_path: String) -> AppResult<ArchiveSummary> {
    Ok(read_archive(&source_path)?.summary())
}

/// 从归档导入
#[tauri::command]
pub fn import_library(
    db: State<'_, Db>,
    paths: State<'_, AppPaths>,
    source_path: String,
    mode: String,
) -> AppResult<ImportOutcome> {
    let archive = read_archive(&source_path)?;
    let mode = ImportMode::parse(&mode)?;

    // 替换模式会清空现有库，先留一份数据库副本
    let safety = if mode == ImportMode::Replace {
        Some(safety_copy(&db, &paths)?)
    } else {
        None
    };

    let mut outcome = library::import(&db, &archive, mode)?;
    outcome.safety_backup = safety;

    log::info!(
        "游戏库导入完成（{}）：新增 {} 个游戏，跳过 {} 个",
        outcome.mode,
        outcome.imported_games,
        outcome.skipped_games
    );
    Ok(outcome)
}
