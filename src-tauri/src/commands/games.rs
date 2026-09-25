//! 游戏库、分类、标签相关命令。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::paths::{sanitize_file_name, AppPaths};
use std::path::Path;
use tauri::State;

// ==================== 游戏 ====================

#[tauri::command]
pub fn list_games(db: State<'_, Db>, filter: GameFilter) -> AppResult<Vec<Game>> {
    db.list_games(&filter)
}

#[tauri::command]
pub fn get_game(db: State<'_, Db>, id: i64) -> AppResult<Game> {
    db.get_game(id)
}

#[tauri::command]
pub fn create_game(db: State<'_, Db>, input: GameInput) -> AppResult<i64> {
    validate_input(&input)?;
    db.create_game(&input)
}

#[tauri::command]
pub fn update_game(db: State<'_, Db>, id: i64, input: GameInput) -> AppResult<()> {
    validate_input(&input)?;
    db.update_game(id, &input)
}

/// 入参校验：标题必填，状态必须是合法枚举值。
fn validate_input(input: &GameInput) -> AppResult<()> {
    if input.title.trim().is_empty() {
        return Err(AppError::msg("游戏名不能为空"));
    }
    if let Some(status) = input.play_status.as_deref() {
        if !is_valid_status(status) {
            return Err(AppError::msg(format!("非法的游玩状态: {status}")));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn delete_game(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.delete_game(id)
}

/// 批量导入扫描结果
#[tauri::command]
pub fn import_games(db: State<'_, Db>, inputs: Vec<GameInput>) -> AppResult<Vec<i64>> {
    let requested = inputs.len();
    let mut ids = Vec::with_capacity(requested);
    for input in inputs {
        if input.title.trim().is_empty() {
            continue;
        }
        match db.create_game(&input) {
            Ok(id) => ids.push(id),
            Err(error) => log::warn!("导入失败 title={}: {}", input.title, error),
        }
    }
    log::info!("批量导入完成 请求={} 成功={}", requested, ids.len());
    Ok(ids)
}

#[tauri::command]
pub fn batch_set_category(
    db: State<'_, Db>,
    ids: Vec<i64>,
    category_id: Option<i64>,
) -> AppResult<()> {
    db.set_games_category(&ids, category_id)
}

#[tauri::command]
pub fn batch_set_status(db: State<'_, Db>, ids: Vec<i64>, status: String) -> AppResult<()> {
    if !is_valid_status(&status) {
        return Err(AppError::msg(format!("非法的游玩状态: {status}")));
    }
    db.set_games_status(&ids, &status)
}

#[tauri::command]
pub fn batch_set_favorite(db: State<'_, Db>, ids: Vec<i64>, favorite: bool) -> AppResult<()> {
    db.set_games_favorite(&ids, favorite)
}

#[tauri::command]
pub fn batch_add_tags(db: State<'_, Db>, ids: Vec<i64>, tags: Vec<String>) -> AppResult<()> {
    db.add_tags_to_games(&ids, &tags)
}

#[tauri::command]
pub fn reorder_games(db: State<'_, Db>, ids: Vec<i64>) -> AppResult<()> {
    db.reorder_games(&ids)
}

/// 设置封面：把用户选择的图片复制进应用数据目录，避免原图被移动后失效。
#[tauri::command]
pub fn set_game_cover(
    db: State<'_, Db>,
    paths: State<'_, AppPaths>,
    game_id: i64,
    source_path: String,
) -> AppResult<String> {
    let source = Path::new(&source_path);
    if !source.is_file() {
        return Err(AppError::msg(format!("图片不存在: {source_path}")));
    }
    let extension = source
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "jpg".to_string());
    let allowed = ["jpg", "jpeg", "png", "webp", "bmp", "gif"];
    if !allowed.contains(&extension.as_str()) {
        return Err(AppError::msg(format!("不支持的图片格式: {extension}")));
    }

    let game = db.get_game(game_id)?;
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S");
    let file_name = format!(
        "{}_{}.{}",
        game_id,
        stamp,
        extension
    );
    let dest = paths.covers_dir().join(&file_name);
    std::fs::copy(source, &dest)?;

    // 删除该游戏上一张封面，避免缓存目录无限膨胀
    if let Some(previous) = game.cover_path.as_ref().map(std::path::PathBuf::from) {
        if previous.starts_with(paths.covers_dir()) && previous.is_file() {
            let _ = std::fs::remove_file(&previous);
        }
    }

    let dest_str = dest.to_string_lossy().to_string();
    db.update_game(
        game_id,
        &GameInput {
            title: game.title,
            cover_path: Some(dest_str.clone()),
            ..Default::default()
        },
    )?;
    Ok(dest_str)
}

#[tauri::command]
pub fn clear_game_cover(
    db: State<'_, Db>,
    paths: State<'_, AppPaths>,
    game_id: i64,
) -> AppResult<()> {
    let game = db.get_game(game_id)?;
    if let Some(previous) = game.cover_path.as_ref().map(std::path::PathBuf::from) {
        if previous.starts_with(paths.covers_dir()) && previous.is_file() {
            let _ = std::fs::remove_file(&previous);
        }
    }
    db.update_game(
        game_id,
        &GameInput {
            title: game.title,
            cover_path: Some(String::new()),
            ..Default::default()
        },
    )
}

// ==================== 分类 ====================

#[tauri::command]
pub fn list_categories(db: State<'_, Db>) -> AppResult<Vec<Category>> {
    db.list_categories()
}

#[tauri::command]
pub fn create_category(db: State<'_, Db>, name: String, icon: Option<String>) -> AppResult<i64> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("分类名不能为空"));
    }
    db.create_category(trimmed, icon.as_deref())
}

#[tauri::command]
pub fn update_category(
    db: State<'_, Db>,
    id: i64,
    name: String,
    icon: Option<String>,
) -> AppResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("分类名不能为空"));
    }
    db.update_category(id, trimmed, icon.as_deref())
}

#[tauri::command]
pub fn delete_category(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.delete_category(id)
}

// ==================== 标签 ====================

#[tauri::command]
pub fn list_tags(db: State<'_, Db>) -> AppResult<Vec<Tag>> {
    db.list_tags()
}

#[tauri::command]
pub fn update_tag(
    db: State<'_, Db>,
    id: i64,
    name: String,
    color: Option<String>,
) -> AppResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("标签名不能为空"));
    }
    db.update_tag(id, trimmed, color.as_deref())
}

#[tauri::command]
pub fn delete_tag(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.delete_tag(id)
}

/// 给某个游戏覆盖式设置标签
#[tauri::command]
pub fn set_game_tags(db: State<'_, Db>, game_id: i64, tags: Vec<String>) -> AppResult<()> {
    db.with_conn(|conn| Db::set_game_tags(conn, game_id, &tags))
}

// ==================== 其它 ====================

/// 生成封面文件的候选名（供前端保存自定义封面时使用）
#[tauri::command]
pub fn suggest_cover_name(title: String) -> String {
    format!("{}.png", sanitize_file_name(&title))
}
