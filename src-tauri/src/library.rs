//! 游戏库导入 / 导出（JSON 归档）。
//!
//! 归档只包含**元数据**：分类、标签、游戏记录、游玩会话、存档槽位记录、补丁、
//! 笔记与资源链接。封面图片与存档 zip 归档位于应用数据目录，不进入 JSON，
//! 需要用户另行复制（界面上有说明）。
//!
//! 导入时所有主键都会**重新分配**，因此归档内用旧 id 建立的关联
//! （game_id / category_id）必须通过映射表重写，否则会串到别的记录上。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::*;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 归档格式标识，用于识别文件是否为本应用导出。
pub const FORMAT: &str = "galmanager-library";

/// 归档结构版本。导入低版本归档始终允许；高于当前版本则拒绝。
pub const VERSION: i32 = 1;

// ==================== 归档结构 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryArchive {
    pub format: String,
    pub version: i32,
    pub exported_at: String,
    pub app_version: String,
    pub categories: Vec<Category>,
    pub tags: Vec<Tag>,
    /// 每条游戏内含 tags（名字），用于重建多对多关系
    pub games: Vec<Game>,
    pub sessions: Vec<PlaySession>,
    pub save_slots: Vec<SaveSlot>,
    pub patches: Vec<Patch>,
    pub notes: Vec<Note>,
    pub links: Vec<ResourceLink>,
}

/// 归档内容摘要（导入前预览用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSummary {
    pub format: String,
    pub version: i32,
    pub exported_at: String,
    pub app_version: String,
    pub game_count: usize,
    pub category_count: usize,
    pub tag_count: usize,
    pub session_count: usize,
    pub save_slot_count: usize,
    pub patch_count: usize,
    pub note_count: usize,
    pub link_count: usize,
}

/// 导出结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutcome {
    pub path: String,
    pub bytes: i64,
    pub summary: ArchiveSummary,
}

/// 导入结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub mode: String,
    pub imported_games: usize,
    /// 合并模式下因已存在而跳过的游戏数
    pub skipped_games: usize,
    pub categories: usize,
    pub tags: usize,
    pub sessions: usize,
    pub save_slots: usize,
    pub patches: usize,
    pub notes: usize,
    pub links: usize,
    /// 替换模式下的数据库安全副本路径
    pub safety_backup: Option<String>,
}

/// 导入模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    /// 合并：保留现有数据，跳过已存在的游戏
    Merge,
    /// 替换：清空全部库数据后导入
    Replace,
}

impl ImportMode {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "merge" => Ok(Self::Merge),
            "replace" => Ok(Self::Replace),
            other => Err(AppError::msg(format!("未知的导入模式: {other}"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Replace => "replace",
        }
    }
}

impl LibraryArchive {
    pub fn summary(&self) -> ArchiveSummary {
        ArchiveSummary {
            format: self.format.clone(),
            version: self.version,
            exported_at: self.exported_at.clone(),
            app_version: self.app_version.clone(),
            game_count: self.games.len(),
            category_count: self.categories.len(),
            tag_count: self.tags.len(),
            session_count: self.sessions.len(),
            save_slot_count: self.save_slots.len(),
            patch_count: self.patches.len(),
            note_count: self.notes.len(),
            link_count: self.links.len(),
        }
    }

    /// 校验归档是否可被当前版本导入。
    pub fn validate(&self) -> AppResult<()> {
        if self.format != FORMAT {
            return Err(AppError::msg(format!(
                "不是 GalManager 归档文件（format = {}）",
                self.format
            )));
        }
        if self.version > VERSION {
            return Err(AppError::msg(format!(
                "归档版本 {} 高于当前支持的 {}，请先升级应用",
                self.version, VERSION
            )));
        }
        Ok(())
    }
}

// ==================== 导出 ====================

pub fn export(db: &Db) -> AppResult<LibraryArchive> {
    Ok(LibraryArchive {
        format: FORMAT.to_string(),
        version: VERSION,
        exported_at: chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        categories: db.list_categories()?,
        tags: db.list_tags()?,
        // 默认过滤器按标题升序，保证导出结果稳定可比对
        games: db.list_games(&GameFilter::default())?,
        sessions: db.all_sessions()?,
        save_slots: db.all_save_slots()?,
        patches: db.all_patches()?,
        notes: db.all_notes()?,
        links: db.list_links(None)?,
    })
}

// ==================== 导入 ====================

/// 目录路径归一化：统一分隔符、去掉尾部反斜杠、转小写，用于去重比对。
fn normalize_key(path: &str) -> String {
    path.trim()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

pub fn import(db: &Db, archive: &LibraryArchive, mode: ImportMode) -> AppResult<ImportOutcome> {
    archive.validate()?;

    db.with_tx(|tx| {
        let mut outcome = ImportOutcome {
            mode: mode.as_str().to_string(),
            ..Default::default()
        };

        if mode == ImportMode::Replace {
            // 顺序无关紧要（外键级联会兜底），但先删子表更直观
            for table in [
                "resource_links",
                "notes",
                "patches",
                "save_slots",
                "play_sessions",
                "game_tags",
                "games",
                "tags",
                "categories",
            ] {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
        }

        // ---- 分类：按名称复用，记录 旧id -> 新id ----
        let mut category_map: HashMap<i64, i64> = HashMap::new();
        for category in &archive.categories {
            let name = category.name.trim();
            if name.is_empty() {
                continue;
            }
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM categories WHERE name = ?1",
                    params![name],
                    |row| row.get(0),
                )
                .optional()?;
            let id = match existing {
                Some(id) => id,
                None => {
                    tx.execute(
                        "INSERT INTO categories(name, icon, sort_order) VALUES (?1, ?2, ?3)",
                        params![name, category.icon, category.sort_order],
                    )?;
                    outcome.categories += 1;
                    tx.last_insert_rowid()
                }
            };
            category_map.insert(category.id, id);
        }

        // ---- 标签：按名称复用（可能包含没挂到任何游戏上的孤立标签）----
        for tag in &archive.tags {
            let name = tag.name.trim();
            if name.is_empty() {
                continue;
            }
            let existed: bool = tx
                .query_row("SELECT 1 FROM tags WHERE name = ?1", params![name], |row| {
                    row.get::<_, i64>(0)
                })
                .optional()?
                .is_some();
            Db::ensure_tag(tx, name, tag.color.as_deref())?;
            if !existed {
                outcome.tags += 1;
            }
        }

        // ---- 合并模式下预读现有游戏，用于去重 ----
        let mut existing_paths: HashSet<String> = HashSet::new();
        let mut existing_titles: HashSet<String> = HashSet::new();
        if mode == ImportMode::Merge {
            let mut stmt = tx.prepare("SELECT path, title FROM games")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (path, title) = row?;
                if let Some(path) = path {
                    let key = normalize_key(&path);
                    if !key.is_empty() {
                        existing_paths.insert(key);
                    }
                }
                existing_titles.insert(title.trim().to_lowercase());
            }
        }

        // ---- 游戏 ----
        let mut game_map: HashMap<i64, i64> = HashMap::new();
        let mut sort_order: i64 = tx
            .query_row("SELECT COALESCE(MAX(sort_order), 0) FROM games", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        for game in &archive.games {
            let title = game.title.trim();
            if title.is_empty() {
                continue;
            }

            if mode == ImportMode::Merge {
                let path_key = game
                    .path
                    .as_deref()
                    .map(normalize_key)
                    .filter(|key| !key.is_empty());
                // 有路径就按路径判重（更可靠）；没路径退回按标题判重
                let duplicated = match &path_key {
                    Some(key) => existing_paths.contains(key),
                    None => existing_titles.contains(&title.to_lowercase()),
                };
                if duplicated {
                    outcome.skipped_games += 1;
                    continue;
                }
            }

            sort_order += 1;
            let category_id = game
                .category_id
                .and_then(|old| category_map.get(&old).copied());
            let play_status = if is_valid_status(&game.play_status) {
                game.play_status.clone()
            } else {
                STATUS_UNPLAYED.to_string()
            };

            tx.execute(
                "INSERT INTO games(
                    title, original_title, path, executable, args, cover_path, engine, engine_confidence,
                    category_id, play_status, favorite, rating, le_launch, le_locale,
                    total_play_seconds, last_played_at, release_date, developer, description, notes,
                    save_path, sort_order, search_index, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,
                         COALESCE(NULLIF(?24,''), datetime('now','localtime')),
                         COALESCE(NULLIF(?25,''), datetime('now','localtime')))",
                params![
                    title,
                    game.original_title,
                    game.path,
                    game.executable,
                    game.args,
                    game.cover_path,
                    game.engine,
                    game.engine_confidence,
                    category_id,
                    play_status,
                    game.favorite,
                    game.rating,
                    game.le_launch,
                    game.le_locale,
                    game.total_play_seconds,
                    game.last_played_at,
                    game.release_date,
                    game.developer,
                    game.description,
                    game.notes,
                    game.save_path,
                    sort_order,
                    crate::search::build_search_index(
                        title,
                        game.original_title.as_deref(),
                        game.developer.as_deref(),
                    ),
                    game.created_at,
                    game.updated_at,
                ],
            )?;

            let new_id = tx.last_insert_rowid();
            game_map.insert(game.id, new_id);
            outcome.imported_games += 1;

            let tag_names: Vec<String> = game.tags.iter().map(|t| t.name.clone()).collect();
            if !tag_names.is_empty() {
                Db::set_game_tags(tx, new_id, &tag_names)?;
            }

            // 同一次导入内也要防重：归档自身若含重复路径，只保留第一条
            if let Some(key) = game.path.as_deref().map(normalize_key) {
                if !key.is_empty() {
                    existing_paths.insert(key);
                }
            }
            existing_titles.insert(title.to_lowercase());
        }

        // ---- 子表：全部按 game_map 重写 game_id ----
        for session in &archive.sessions {
            let Some(game_id) = game_map.get(&session.game_id).copied() else {
                continue;
            };
            tx.execute(
                "INSERT INTO play_sessions(game_id, started_at, ended_at, duration_seconds, note)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    game_id,
                    session.started_at,
                    session.ended_at,
                    session.duration_seconds,
                    session.note
                ],
            )?;
            outcome.sessions += 1;
        }

        for slot in &archive.save_slots {
            let Some(game_id) = game_map.get(&slot.game_id).copied() else {
                continue;
            };
            tx.execute(
                "INSERT INTO save_slots(game_id, slot_name, engine, source_path, backup_path,
                                        size_bytes, file_count, remark, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,
                         COALESCE(NULLIF(?9,''), datetime('now','localtime')))",
                params![
                    game_id,
                    slot.slot_name,
                    slot.engine,
                    slot.source_path,
                    slot.backup_path,
                    slot.size_bytes,
                    slot.file_count,
                    slot.remark,
                    slot.created_at
                ],
            )?;
            outcome.save_slots += 1;
        }

        for patch in &archive.patches {
            let Some(game_id) = game_map.get(&patch.game_id).copied() else {
                continue;
            };
            tx.execute(
                "INSERT INTO patches(game_id, name, version, patch_type, file_path, url,
                                     installed, remark, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,
                         COALESCE(NULLIF(?9,''), datetime('now','localtime')))",
                params![
                    game_id,
                    patch.name,
                    patch.version,
                    patch.patch_type,
                    patch.file_path,
                    patch.url,
                    patch.installed,
                    patch.remark,
                    patch.created_at
                ],
            )?;
            outcome.patches += 1;
        }

        for note in &archive.notes {
            let Some(game_id) = game_map.get(&note.game_id).copied() else {
                continue;
            };
            tx.execute(
                "INSERT INTO notes(game_id, title, content, updated_at, created_at)
                 VALUES (?1,?2,?3,
                         COALESCE(NULLIF(?4,''), datetime('now','localtime')),
                         COALESCE(NULLIF(?5,''), datetime('now','localtime')))",
                params![game_id, note.title, note.content, note.updated_at, note.created_at],
            )?;
            outcome.notes += 1;
        }

        for link in &archive.links {
            // 链接的 game_id 可为空（全局收藏）
            let game_id = match link.game_id {
                Some(old) => match game_map.get(&old).copied() {
                    Some(id) => Some(id),
                    None => continue,
                },
                None => None,
            };
            tx.execute(
                "INSERT INTO resource_links(game_id, title, url, kind, remark, created_at)
                 VALUES (?1,?2,?3,?4,?5,
                         COALESCE(NULLIF(?6,''), datetime('now','localtime')))",
                params![
                    game_id,
                    link.title,
                    link.url,
                    link.kind,
                    link.remark,
                    link.created_at
                ],
            )?;
            outcome.links += 1;
        }

        Ok(outcome)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GameInput;
    use std::path::{Path, PathBuf};

    fn temp_db(tag: &str) -> (Db, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "galmanager_libtest_{tag}_{}.db",
            std::process::id()
        ));
        cleanup(&path);
        (Db::open(&path).expect("打开临时数据库失败"), path)
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_file(path);
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    /// 造一个内容比较丰富的库，覆盖各类子表
    fn seed(db: &Db) -> i64 {
        let category_id = db.create_category("视觉小说", None).unwrap();
        let game_id = db
            .create_game(&GameInput {
                title: "千恋万花".to_string(),
                original_title: Some("Senren * Banka".to_string()),
                developer: Some("柚子社".to_string()),
                path: Some(r"D:\Games\千恋万花".to_string()),
                category_id: Some(category_id),
                play_status: Some(STATUS_PLAYING.to_string()),
                favorite: Some(1),
                rating: Some(90),
                tags: Some(vec!["纯爱".to_string(), "和风".to_string()]),
                ..Default::default()
            })
            .unwrap();
        db.insert_session(game_id, "2026-01-02 20:00:00", "2026-01-02 22:00:00", 7200)
            .unwrap();
        db.insert_save_slot(
            game_id,
            "手动存档",
            Some("kirikiri"),
            r"C:\saves\senren",
            r"D:\backup\senren.zip",
            1024,
            3,
            Some("一周目"),
        )
        .unwrap();
        db.upsert_patch(
            None,
            game_id,
            "官方汉化",
            Some("1.1"),
            "translation",
            None,
            Some("https://example.com/patch"),
            false,
            None,
        )
        .unwrap();
        db.upsert_note(None, game_id, "攻略", "注意选项 3").unwrap();
        db.insert_link(
            Some(game_id),
            "日文 Wiki",
            "https://example.com/wiki",
            "wiki",
            None,
        )
        .unwrap();
        game_id
    }

    #[test]
    fn export_then_import_into_empty_db_restores_everything() {
        let (source, source_path) = temp_db("roundtrip_src");
        seed(&source);
        let archive = export(&source).expect("导出失败");
        assert_eq!(archive.games.len(), 1);
        assert_eq!(archive.categories.len(), 1);
        assert_eq!(archive.tags.len(), 2);

        let (target, target_path) = temp_db("roundtrip_dst");
        let outcome = import(&target, &archive, ImportMode::Merge).expect("导入失败");
        assert_eq!(outcome.imported_games, 1);
        assert_eq!(outcome.skipped_games, 0);
        assert_eq!(outcome.sessions, 1);
        assert_eq!(outcome.save_slots, 1);
        assert_eq!(outcome.patches, 1);
        assert_eq!(outcome.notes, 1);
        assert_eq!(outcome.links, 1);

        let games = target.list_games(&GameFilter::default()).unwrap();
        assert_eq!(games.len(), 1);
        let game = &games[0];
        assert_eq!(game.title, "千恋万花");
        assert_eq!(game.favorite, 1);
        assert_eq!(game.rating, 90);
        // 分类与标签的关联必须跟着重建
        assert!(game.category_id.is_some(), "分类关联丢失");
        let mut tag_names: Vec<&str> = game.tags.iter().map(|t| t.name.as_str()).collect();
        tag_names.sort();
        assert_eq!(tag_names, ["和风", "纯爱"]);
        // 子表按新 id 挂载
        assert_eq!(target.sessions_of_game(game.id, 10).unwrap().len(), 1);
        assert_eq!(target.list_save_slots(game.id).unwrap().len(), 1);
        assert_eq!(target.list_patches(game.id).unwrap().len(), 1);
        assert_eq!(target.list_notes(game.id).unwrap().len(), 1);
        assert_eq!(target.list_links(Some(game.id)).unwrap().len(), 1);

        cleanup(&source_path);
        cleanup(&target_path);
    }

    #[test]
    fn pinyin_index_is_rebuilt_on_import() {
        let (source, source_path) = temp_db("pinyin_src");
        seed(&source);
        let archive = export(&source).unwrap();

        let (target, target_path) = temp_db("pinyin_dst");
        import(&target, &archive, ImportMode::Merge).unwrap();

        let filter = GameFilter {
            keyword: "qlwh".to_string(),
            ..Default::default()
        };
        let found = target.list_games(&filter).unwrap();
        assert_eq!(found.len(), 1, "导入后拼音索引应可检索");
        assert_eq!(found[0].title, "千恋万花");

        cleanup(&source_path);
        cleanup(&target_path);
    }

    #[test]
    fn merge_skips_games_that_already_exist() {
        let (source, source_path) = temp_db("merge_src");
        seed(&source);
        let archive = export(&source).unwrap();

        let (target, target_path) = temp_db("merge_dst");
        // 目标库已有同一个路径的游戏
        target
            .create_game(&GameInput {
                title: "千恋万花".to_string(),
                path: Some(r"d:/Games/千恋万花/".to_string()), // 大小写 + 斜杠差异也应判为同一个
                ..Default::default()
            })
            .unwrap();

        let outcome = import(&target, &archive, ImportMode::Merge).unwrap();
        assert_eq!(outcome.imported_games, 0);
        assert_eq!(outcome.skipped_games, 1);
        assert_eq!(target.list_games(&GameFilter::default()).unwrap().len(), 1);

        cleanup(&source_path);
        cleanup(&target_path);
    }

    #[test]
    fn replace_mode_wipes_existing_library_first() {
        let (source, source_path) = temp_db("replace_src");
        seed(&source);
        let archive = export(&source).unwrap();

        let (target, target_path) = temp_db("replace_dst");
        target
            .create_game(&GameInput {
                title: "会被清掉的旧游戏".to_string(),
                path: Some(r"E:\old".to_string()),
                ..Default::default()
            })
            .unwrap();
        target.create_category("旧分类", None).unwrap();

        let outcome = import(&target, &archive, ImportMode::Replace).unwrap();
        assert_eq!(outcome.imported_games, 1);

        let games = target.list_games(&GameFilter::default()).unwrap();
        assert_eq!(games.len(), 1, "替换模式应只剩归档里的游戏");
        assert_eq!(games[0].title, "千恋万花");
        let categories = target.list_categories().unwrap();
        assert_eq!(categories.len(), 1);
        assert_eq!(categories[0].name, "视觉小说");

        cleanup(&source_path);
        cleanup(&target_path);
    }

    #[test]
    fn import_rejects_foreign_or_too_new_archives() {
        let (db, path) = temp_db("reject");

        let mut archive = export(&db).unwrap();
        archive.format = "something-else".to_string();
        let error = import(&db, &archive, ImportMode::Merge).unwrap_err();
        assert!(error.to_string().contains("不是 GalManager 归档文件"));

        let mut archive = export(&db).unwrap();
        archive.version = VERSION + 1;
        let error = import(&db, &archive, ImportMode::Merge).unwrap_err();
        assert!(error.to_string().contains("高于当前支持"));

        cleanup(&path);
    }

    #[test]
    fn import_mode_parsing() {
        assert_eq!(ImportMode::parse("merge").unwrap(), ImportMode::Merge);
        assert_eq!(ImportMode::parse("replace").unwrap(), ImportMode::Replace);
        assert!(ImportMode::parse("whatever").is_err());
    }
}
