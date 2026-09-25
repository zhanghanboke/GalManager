//! 辅助工具命令：汉化补丁管理、攻略笔记、资源链接收藏。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::{Note, Patch, ResourceLink};
use tauri::State;

// ==================== 汉化补丁 ====================

#[tauri::command]
pub fn list_patches(db: State<'_, Db>, game_id: i64) -> AppResult<Vec<Patch>> {
    db.list_patches(game_id)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_patch(
    db: State<'_, Db>,
    id: Option<i64>,
    game_id: i64,
    name: String,
    version: Option<String>,
    patch_type: String,
    file_path: Option<String>,
    url: Option<String>,
    installed: bool,
    remark: Option<String>,
) -> AppResult<i64> {
    if name.trim().is_empty() {
        return Err(AppError::msg("补丁名称不能为空"));
    }
    db.upsert_patch(
        id,
        game_id,
        name.trim(),
        version.as_deref(),
        &patch_type,
        file_path.as_deref(),
        url.as_deref(),
        installed,
        remark.as_deref(),
    )
}

#[tauri::command]
pub fn delete_patch(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.delete_patch(id)
}

/// 切换补丁安装状态
#[tauri::command]
pub fn toggle_patch_installed(db: State<'_, Db>, id: i64, installed: bool) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE patches SET installed = ?1 WHERE id = ?2",
            rusqlite::params![if installed { 1 } else { 0 }, id],
        )?;
        Ok(())
    })
}

// ==================== 攻略笔记 ====================

#[tauri::command]
pub fn list_notes(db: State<'_, Db>, game_id: i64) -> AppResult<Vec<Note>> {
    db.list_notes(game_id)
}

#[tauri::command]
pub fn save_note(
    db: State<'_, Db>,
    id: Option<i64>,
    game_id: i64,
    title: String,
    content: String,
) -> AppResult<i64> {
    let title = if title.trim().is_empty() {
        "未命名笔记".to_string()
    } else {
        title.trim().to_string()
    };
    db.upsert_note(id, game_id, &title, &content)
}

#[tauri::command]
pub fn delete_note(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.delete_note(id)
}

// ==================== 资源链接 ====================

#[tauri::command]
pub fn list_links(db: State<'_, Db>, game_id: Option<i64>) -> AppResult<Vec<ResourceLink>> {
    db.list_links(game_id)
}

#[tauri::command]
pub fn add_link(
    db: State<'_, Db>,
    game_id: Option<i64>,
    title: String,
    url: String,
    kind: String,
    remark: Option<String>,
) -> AppResult<i64> {
    if url.trim().is_empty() {
        return Err(AppError::msg("链接地址不能为空"));
    }
    if !url.trim().starts_with("http://") && !url.trim().starts_with("https://") {
        return Err(AppError::msg("仅支持 http/https 链接"));
    }
    let title = if title.trim().is_empty() {
        url.clone()
    } else {
        title.trim().to_string()
    };
    db.insert_link(game_id, &title, url.trim(), &kind, remark.as_deref())
}

#[tauri::command]
pub fn delete_link(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.delete_link(id)
}
