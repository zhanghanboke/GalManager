//! 目录扫描与引擎识别命令。

use crate::db::Db;
use crate::engine;
use crate::error::{AppError, AppResult};
use crate::models::{EngineInfo, GameInput, ScanCandidate, ScanOptions};
use crate::scanner;
use std::path::PathBuf;
use tauri::State;

/// 扫描目录，返回候选游戏列表。
///
/// 文件系统遍历是阻塞 I/O，放到 `spawn_blocking` 中执行，
/// 避免阻塞 Tauri 的异步运行时线程。
#[tauri::command]
pub async fn scan_directory(
    db: State<'_, Db>,
    options: ScanOptions,
) -> AppResult<Vec<ScanCandidate>> {
    let options_clone = options.clone();
    let mut candidates = tauri::async_runtime::spawn_blocking(move || scanner::scan(&options_clone))
        .await
        .map_err(|error| AppError::msg(format!("扫描任务异常: {error}")))??;

    // 标记已在库中的路径，避免重复导入
    let existing = db.existing_game_paths()?;
    let index = scanner::ImportedPathIndex::new(existing);
    for candidate in &mut candidates {
        candidate.already_imported = index.contains(&PathBuf::from(&candidate.path));
    }

    Ok(candidates)
}

/// 对单个目录做引擎识别（详情页「重新识别」按钮）
#[tauri::command]
pub fn detect_engine(path: String) -> AppResult<Option<EngineInfo>> {
    Ok(engine::detect(&PathBuf::from(path)))
}

/// 列出全部受支持引擎
#[tauri::command]
pub fn list_engines() -> Vec<engine::EngineDescriptor> {
    engine::all_engines()
}

/// 列出某目录下的可执行启动项
#[tauri::command]
pub fn list_executables(path: String) -> AppResult<Vec<String>> {
    scanner::list_executables(&PathBuf::from(path))
}

/// 把扫描结果直接转成入库入参，前端只需补充标题等元数据
#[tauri::command]
pub fn build_game_inputs(candidates: Vec<ScanCandidate>) -> Vec<GameInput> {
    candidates
        .into_iter()
        .map(|candidate| GameInput {
            title: candidate.name,
            path: Some(candidate.path),
            executable: candidate.executables.first().cloned(),
            engine: candidate.engine,
            engine_confidence: Some(candidate.engine_confidence),
            le_launch: Some(0),
            ..Default::default()
        })
        .collect()
}
