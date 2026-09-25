//! 目录扫描与引擎识别命令。

use crate::db::Db;
use crate::engine;
use crate::error::{AppError, AppResult};
use crate::models::{
    DuplicateHint, EngineInfo, Game, GameFilter, GameInput, ScanCandidate, ScanOptions,
};
use crate::scanner;
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::State;

/// 标题归一化：去掉空白并转小写，用于「可能重复」判定。
///
/// 扫描出来的名字已经过目录名清洗，这里再抹掉空白差异
/// （例如「千恋万花」与「千恋 万花」），减少漏判。
fn normalize_title(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// 给扫描结果标注「已在库中」与「可能重复」。
///
/// 抽成纯函数（只依赖传入的数据，不碰数据库），便于单元测试。
fn annotate_duplicates(
    candidates: &mut [ScanCandidate],
    imported_paths: Vec<String>,
    library: &[Game],
) {
    // ---- 1. 路径判定：候选目录落在已入库目录之内（含自身）----
    // ImportedPathIndex 会做大小写与分隔符归一化，因此 `d:/games/x` 与 `D:\Games\X` 能识别为同一个。
    let index = scanner::ImportedPathIndex::new(imported_paths);
    for candidate in candidates.iter_mut() {
        candidate.already_imported = index.contains(&PathBuf::from(&candidate.path));
    }

    // ---- 2. 标题判定：路径不同但标题与库中某条一致，提示「可能重复」----
    // 游戏目录被改名 / 换盘后路径判定会失效，标题是最后一道防线。
    let mut by_title: HashMap<String, &Game> = HashMap::new();
    for game in library {
        by_title
            .entry(normalize_title(&game.title))
            .or_insert(game);
    }

    for candidate in candidates.iter_mut() {
        // 路径已经确认在库中就不必再提示标题重复
        if candidate.already_imported {
            continue;
        }
        let key = normalize_title(&candidate.name);
        if key.is_empty() {
            continue;
        }
        if let Some(game) = by_title.get(&key) {
            candidate.possible_duplicate = Some(DuplicateHint {
                game_id: game.id,
                title: game.title.clone(),
                path: game.path.clone(),
            });
        }
    }
}

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

    let imported_paths = db.existing_game_paths()?;
    let library = db.list_games(&GameFilter::default())?;
    annotate_duplicates(&mut candidates, imported_paths, &library);

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

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(name: &str, path: &str) -> ScanCandidate {
        ScanCandidate {
            name: name.to_string(),
            path: path.to_string(),
            executables: Vec::new(),
            engine: None,
            engine_confidence: 0,
            already_imported: false,
            possible_duplicate: None,
        }
    }

    fn game(id: i64, title: &str, path: &str) -> Game {
        Game {
            id,
            title: title.to_string(),
            original_title: None,
            path: Some(path.to_string()),
            executable: None,
            args: None,
            cover_path: None,
            engine: None,
            engine_confidence: 0,
            category_id: None,
            play_status: "unplayed".to_string(),
            favorite: 0,
            rating: -1,
            le_launch: 0,
            le_locale: None,
            total_play_seconds: 0,
            last_played_at: None,
            release_date: None,
            developer: None,
            description: None,
            notes: None,
            save_path: None,
            sort_order: 0,
            created_at: String::new(),
            updated_at: String::new(),
            tags: Vec::new(),
            session_count: 0,
            save_count: 0,
        }
    }

    #[test]
    fn same_path_is_marked_imported() {
        let mut items = vec![
            candidate("千恋万花", r"D:\Games\千恋万花"),
            candidate("新游戏", r"D:\Games\新游戏"),
        ];
        annotate_duplicates(&mut items, vec![r"D:\Games\千恋万花".to_string()], &[]);
        assert!(items[0].already_imported);
        assert!(!items[1].already_imported);
    }

    /// Windows 上路径大小写与分隔符不敏感，必须识别为同一个目录
    #[test]
    fn path_match_ignores_case_and_separators() {
        let mut items = vec![candidate("千恋万花", r"D:\Games\千恋万花")];
        annotate_duplicates(&mut items, vec!["d:/games/千恋万花/".to_string()], &[]);
        assert!(items[0].already_imported, "大小写与斜杠差异不应漏判");
    }

    /// 子目录也算已在库中：避免把一个游戏的内层目录再导入一遍
    #[test]
    fn subdirectory_of_imported_game_counts_as_imported() {
        let mut items = vec![candidate("千恋万花", r"D:\Games\千恋万花\sub\deep")];
        annotate_duplicates(&mut items, vec![r"D:\Games\千恋万花".to_string()], &[]);
        assert!(items[0].already_imported);
    }

    /// 换了目录但标题一样 → 提示可能重复，但不阻止导入
    #[test]
    fn same_title_at_other_path_is_flagged_as_possible_duplicate() {
        let mut items = vec![candidate("白色相簿2", r"F:\NewDrive\白色相簿2")];
        let library = vec![game(7, "白色相簿2", r"E:\OldDrive\白色相簿2")];
        annotate_duplicates(&mut items, vec![], &library);

        assert!(!items[0].already_imported, "路径不同不应算作已入库");
        let hint = items[0].possible_duplicate.as_ref().expect("应给出重复提示");
        assert_eq!(hint.game_id, 7);
        assert_eq!(hint.title, "白色相簿2");
        assert_eq!(hint.path.as_deref(), Some(r"E:\OldDrive\白色相簿2"));
    }

    /// 标题里的空白与大小写差异不该造成漏判
    #[test]
    fn title_match_ignores_whitespace_and_case() {
        let mut items = vec![candidate("Summer  Pockets", r"F:\x")];
        let library = vec![game(3, "summer pockets", r"E:\y")];
        annotate_duplicates(&mut items, vec![], &library);
        assert!(items[0].possible_duplicate.is_some());
    }

    /// 已在库中的项不再重复给「可能重复」提示，避免同一行出现两种标注
    #[test]
    fn imported_candidates_skip_title_hint() {
        let mut items = vec![candidate("千恋万花", r"D:\Games\千恋万花")];
        let library = vec![game(1, "千恋万花", r"D:\Games\千恋万花")];
        annotate_duplicates(&mut items, vec![r"D:\Games\千恋万花".to_string()], &library);
        assert!(items[0].already_imported);
        assert!(items[0].possible_duplicate.is_none());
    }

    #[test]
    fn unrelated_candidates_get_no_flags() {
        let mut items = vec![candidate("全新游戏", r"D:\Games\全新游戏")];
        let library = vec![game(1, "别的游戏", r"E:\other")];
        annotate_duplicates(&mut items, vec![r"D:\Games\other".to_string()], &library);
        assert!(!items[0].already_imported);
        assert!(items[0].possible_duplicate.is_none());
    }
}
