//! SQLite 持久化层。
//!
//! 设计取舍：桌面单机场景下并发写入极低，因此使用「单连接 + Mutex」而非连接池，
//! 既避免了 `SQLITE_BUSY`，也让事务边界清晰可控。

use crate::error::{AppError, AppResult};
use crate::models::*;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::Arc;

/// 数据库句柄，作为 Tauri 的全局 State 注入。
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// 打开（或创建）数据库并执行建表 / 迁移。
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // WAL 提升读写并发；foreign_keys 让级联删除生效
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    /// 建表与增量迁移。全部使用 `IF NOT EXISTS`，可重复执行。
    fn migrate(&self) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute_batch(SCHEMA)?;
        // 增量迁移：老库没有 search_index 列，需要补列并回填
        self.ensure_search_index(&conn)?;
        // 记录 schema 版本，方便后续做破坏性迁移
        conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    /// 确保 `games.search_index` 存在。
    ///
    /// SQLite 不支持 `ADD COLUMN IF NOT EXISTS`，因此先查 `PRAGMA table_info`；
    /// 对老库补列后立即回填一遍拼音索引，保证升级后搜索立即可用。
    fn ensure_search_index(&self, conn: &Connection) -> AppResult<()> {
        let has_column = {
            let mut stmt = conn.prepare("PRAGMA table_info(games)")?;
            let names = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            names.iter().any(|name| name == "search_index")
        };
        if has_column {
            return Ok(());
        }

        conn.execute("ALTER TABLE games ADD COLUMN search_index TEXT", [])?;
        log::info!("已为 games 表补充 search_index 列，开始回填拼音索引");
        let count = self.rebuild_search_index(conn)?;
        log::info!("拼音索引回填完成，共 {count} 条");
        Ok(())
    }

    /// 重算全部游戏的拼音检索索引，返回处理的条数。
    pub fn rebuild_search_index(&self, conn: &Connection) -> AppResult<usize> {
        let rows: Vec<(i64, String, Option<String>, Option<String>)> = {
            let mut stmt =
                conn.prepare("SELECT id, title, original_title, developer FROM games")?;
            let collected = stmt
                .query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            collected
        };

        for (id, title, original_title, developer) in &rows {
            let index = crate::search::build_search_index(
                title,
                original_title.as_deref(),
                developer.as_deref(),
            );
            conn.execute(
                "UPDATE games SET search_index = ?1 WHERE id = ?2",
                params![index, id],
            )?;
        }
        Ok(rows.len())
    }

    /// 暴露底层连接，供需要事务的复合操作使用。
    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = self.conn.lock();
        f(&conn)
    }

    pub fn with_tx<T>(&self, f: impl FnOnce(&rusqlite::Transaction) -> AppResult<T>) -> AppResult<T> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    }

    // ==================== 设置项 ====================

    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        self.with_conn(|conn| {
            let value = conn
                .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| {
                    row.get::<_, String>(0)
                })
                .optional()?;
            Ok(value)
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
            Ok(())
        })
    }

    pub fn all_settings(&self) -> AppResult<std::collections::HashMap<String, String>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut map = std::collections::HashMap::new();
            for row in rows {
                let (k, v) = row?;
                map.insert(k, v);
            }
            Ok(map)
        })
    }

    // ==================== 分类 ====================

    pub fn list_categories(&self) -> AppResult<Vec<Category>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.name, c.icon, c.sort_order,
                        (SELECT COUNT(*) FROM games g WHERE g.category_id = c.id) AS game_count
                 FROM categories c
                 ORDER BY c.sort_order ASC, c.id ASC",
            )?;
            let rows = stmt.query_map([], map_category)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    pub fn create_category(&self, name: &str, icon: Option<&str>) -> AppResult<i64> {
        self.with_conn(|conn| {
            let order: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM categories",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(1);
            conn.execute(
                "INSERT INTO categories(name, icon, sort_order) VALUES (?1, ?2, ?3)",
                params![name, icon, order],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn update_category(&self, id: i64, name: &str, icon: Option<&str>) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE categories SET name = ?1, icon = ?2 WHERE id = ?3",
                params![name, icon, id],
            )?;
            Ok(())
        })
    }

    pub fn delete_category(&self, id: i64) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM categories WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    // ==================== 标签 ====================

    pub fn list_tags(&self) -> AppResult<Vec<Tag>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.name, t.color,
                        (SELECT COUNT(*) FROM game_tags gt WHERE gt.tag_id = t.id) AS game_count
                 FROM tags t
                 ORDER BY game_count DESC, t.name ASC",
            )?;
            let rows = stmt.query_map([], map_tag)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// 按名称取标签 id，不存在则创建（导入流程高频调用）。
    pub fn ensure_tag(conn: &Connection, name: &str, color: Option<&str>) -> AppResult<i64> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AppError::msg("标签名不能为空"));
        }
        let existing: Option<i64> = conn
            .query_row("SELECT id FROM tags WHERE name = ?1", params![trimmed], |row| {
                row.get(0)
            })
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
        conn.execute(
            "INSERT INTO tags(name, color) VALUES (?1, ?2)",
            params![trimmed, color],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_tag(&self, id: i64, name: &str, color: Option<&str>) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE tags SET name = ?1, color = ?2 WHERE id = ?3",
                params![name, color, id],
            )?;
            Ok(())
        })
    }

    pub fn delete_tag(&self, id: i64) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    /// 覆盖式设置某游戏的标签集合。
    pub fn set_game_tags(conn: &Connection, game_id: i64, tags: &[String]) -> AppResult<()> {
        conn.execute("DELETE FROM game_tags WHERE game_id = ?1", params![game_id])?;
        for name in tags {
            let tag_id = Self::ensure_tag(conn, name, None)?;
            conn.execute(
                "INSERT OR IGNORE INTO game_tags(game_id, tag_id) VALUES (?1, ?2)",
                params![game_id, tag_id],
            )?;
        }
        Ok(())
    }

    pub fn tags_of_game(conn: &Connection, game_id: i64) -> AppResult<Vec<Tag>> {
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.color, 0
             FROM tags t
             JOIN game_tags gt ON gt.tag_id = t.id
             WHERE gt.game_id = ?1
             ORDER BY t.name ASC",
        )?;
        let rows = stmt.query_map(params![game_id], map_tag)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // ==================== 游戏 ====================

    /// 查询游戏列表（含筛选、排序、标签聚合）。
    pub fn list_games(&self, filter: &GameFilter) -> AppResult<Vec<Game>> {
        self.with_conn(|conn| {
            let mut sql = String::from(
                "SELECT g.*, 
                        (SELECT COUNT(*) FROM play_sessions ps WHERE ps.game_id = g.id) AS session_count,
                        (SELECT COUNT(*) FROM save_slots ss WHERE ss.game_id = g.id) AS save_count
                 FROM games g WHERE 1 = 1",
            );
            let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(category_id) = filter.category_id {
                if category_id < 0 {
                    // -1 约定为「未分类」
                    sql.push_str(" AND g.category_id IS NULL");
                } else {
                    sql.push_str(" AND g.category_id = ?");
                    args.push(Box::new(category_id));
                }
            }

            if filter.favorite_only {
                sql.push_str(" AND g.favorite = 1");
            }

            if !filter.statuses.is_empty() {
                let placeholders = vec!["?"; filter.statuses.len()].join(",");
                sql.push_str(&format!(" AND g.play_status IN ({})", placeholders));
                for s in &filter.statuses {
                    args.push(Box::new(s.clone()));
                }
            }

            if !filter.engines.is_empty() {
                let placeholders = vec!["?"; filter.engines.len()].join(",");
                sql.push_str(&format!(" AND g.engine IN ({})", placeholders));
                for e in &filter.engines {
                    args.push(Box::new(e.clone()));
                }
            }

            if !filter.keyword.trim().is_empty() {
                let kw = format!("%{}%", filter.keyword.trim());
                // search_index 是拼音索引（全拼 + 首字母），因此 `qlwh`、`qianlian`
                // 也能命中「千恋万花」；原文匹配仍由前四个 LIKE 负责。
                sql.push_str(
                    " AND (g.title LIKE ? OR IFNULL(g.original_title,'') LIKE ?
                       OR IFNULL(g.developer,'') LIKE ? OR IFNULL(g.path,'') LIKE ?
                       OR IFNULL(g.search_index,'') LIKE ?)",
                );
                for _ in 0..5 {
                    args.push(Box::new(kw.clone()));
                }
            }

            // 标签筛选：全部命中（AND 语义）
            for tag in &filter.tags {
                sql.push_str(
                    " AND EXISTS (SELECT 1 FROM game_tags gt JOIN tags t ON t.id = gt.tag_id
                                  WHERE gt.game_id = g.id AND t.name = ?)",
                );
                args.push(Box::new(tag.clone()));
            }

            let sort_by = filter.sort_by.as_deref().unwrap_or("title");
            let direction = if filter.sort_desc { "DESC" } else { "ASC" };
            let order_clause = match sort_by {
                "lastPlayed" => format!("g.last_played_at IS NULL, g.last_played_at {}", direction),
                "playtime" => format!("g.total_play_seconds {}", direction),
                "created" => format!("g.created_at {}", direction),
                "rating" => format!("g.rating {}", direction),
                "release" => format!("g.release_date IS NULL, g.release_date {}", direction),
                "manual" => format!("g.sort_order {}", direction),
                _ => format!("g.title {} ", direction),
            };
            sql.push_str(" ORDER BY ");
            sql.push_str(&order_clause);

            let mut stmt = conn.prepare(&sql)?;
            let params_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
            let rows = stmt.query_map(params_ref.as_slice(), map_game)?;

            let mut games = Vec::new();
            for row in rows {
                let mut game = row?;
                game.tags = Self::tags_of_game(conn, game.id)?;
                games.push(game);
            }
            Ok(games)
        })
    }

    pub fn get_game(&self, id: i64) -> AppResult<Game> {
        self.with_conn(|conn| {
            let mut game = conn
                .query_row(
                    "SELECT g.*,
                            (SELECT COUNT(*) FROM play_sessions ps WHERE ps.game_id = g.id) AS session_count,
                            (SELECT COUNT(*) FROM save_slots ss WHERE ss.game_id = g.id) AS save_count
                     FROM games g WHERE g.id = ?1",
                    params![id],
                    map_game,
                )
                .optional()?
                .ok_or_else(|| AppError::msg("游戏不存在"))?;
            game.tags = Self::tags_of_game(conn, id)?;
            Ok(game)
        })
    }

    /// 已入库游戏的目录集合，扫描去重用。
    pub fn existing_game_paths(&self) -> AppResult<Vec<String>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT path FROM games WHERE path IS NOT NULL")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// 新增游戏，返回新 id。
    pub fn create_game(&self, input: &GameInput) -> AppResult<i64> {
        self.with_tx(|tx| {
            let order: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM games",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(1);
            tx.execute(
                "INSERT INTO games(
                    title, original_title, path, executable, args, cover_path, engine, engine_confidence,
                    category_id, play_status, favorite, rating, le_launch, le_locale,
                    release_date, developer, description, notes, save_path, sort_order, search_index)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
                params![
                    input.title.trim(),
                    input.original_title,
                    input.path,
                    input.executable,
                    input.args,
                    input.cover_path,
                    input.engine,
                    input.engine_confidence.unwrap_or(0),
                    input.category_id,
                    input.play_status.clone().unwrap_or_else(|| STATUS_UNPLAYED.into()),
                    input.favorite.unwrap_or(0),
                    input.rating.unwrap_or(-1),
                    input.le_launch.unwrap_or(0),
                    input.le_locale.clone().unwrap_or_else(|| "ja_JP".into()),
                    input.release_date,
                    input.developer,
                    input.description,
                    input.notes,
                    input.save_path,
                    order,
                    crate::search::build_search_index(
                        input.title.trim(),
                        input.original_title.as_deref(),
                        input.developer.as_deref(),
                    ),
                ],
            )?;
            let id = tx.last_insert_rowid();
            if let Some(tags) = &input.tags {
                Self::set_game_tags(tx, id, tags)?;
            }
            Ok(id)
        })
    }

    /// 局部更新：只覆盖入参中 `Some` 的字段。
    pub fn update_game(&self, id: i64, input: &GameInput) -> AppResult<()> {
        self.with_tx(|tx| {
            let mut sets: Vec<String> = Vec::new();
            let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            macro_rules! set_field {
                ($field:expr, $value:expr) => {
                    if let Some(v) = $value {
                        sets.push(format!("{} = ?", $field));
                        args.push(Box::new(v));
                    }
                };
            }

            set_field!("title", Some(input.title.trim().to_string()));
            set_field!("original_title", input.original_title.clone());
            set_field!("path", input.path.clone());
            set_field!("executable", input.executable.clone());
            set_field!("args", input.args.clone());
            set_field!("cover_path", input.cover_path.clone());
            set_field!("engine", input.engine.clone());
            set_field!("engine_confidence", input.engine_confidence);
            set_field!("category_id", input.category_id);
            set_field!("play_status", input.play_status.clone());
            set_field!("favorite", input.favorite);
            set_field!("rating", input.rating);
            set_field!("le_launch", input.le_launch);
            set_field!("le_locale", input.le_locale.clone());
            set_field!("release_date", input.release_date.clone());
            set_field!("developer", input.developer.clone());
            set_field!("description", input.description.clone());
            set_field!("notes", input.notes.clone());
            set_field!("save_path", input.save_path.clone());

            if sets.is_empty() && input.tags.is_none() {
                return Ok(());
            }

            if !sets.is_empty() {
                sets.push("updated_at = datetime('now','localtime')".to_string());
                let sql = format!("UPDATE games SET {} WHERE id = ?", sets.join(", "));
                args.push(Box::new(id));
                let params_ref: Vec<&dyn rusqlite::ToSql> =
                    args.iter().map(|b| b.as_ref()).collect();
                tx.execute(&sql, params_ref.as_slice())?;
            }

            if let Some(tags) = &input.tags {
                Self::set_game_tags(tx, id, tags)?;
            }

            // 标题 / 原名 / 开发商可能刚被改过，重算拼音索引。
            // 这里是局部更新，必须基于库里当前的值算，不能只看入参。
            let (title, original_title, developer): (String, Option<String>, Option<String>) = tx
                .query_row(
                    "SELECT title, original_title, developer FROM games WHERE id = ?1",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
            tx.execute(
                "UPDATE games SET search_index = ?1 WHERE id = ?2",
                params![
                    crate::search::build_search_index(
                        &title,
                        original_title.as_deref(),
                        developer.as_deref(),
                    ),
                    id
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete_game(&self, id: i64) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM games WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    /// 批量设置分类
    pub fn set_games_category(&self, ids: &[i64], category_id: Option<i64>) -> AppResult<()> {
        self.with_tx(|tx| {
            for id in ids {
                tx.execute(
                    "UPDATE games SET category_id = ?1, updated_at = datetime('now','localtime') WHERE id = ?2",
                    params![category_id, id],
                )?;
            }
            Ok(())
        })
    }

    /// 批量设置游玩状态
    pub fn set_games_status(&self, ids: &[i64], status: &str) -> AppResult<()> {
        self.with_tx(|tx| {
            for id in ids {
                tx.execute(
                    "UPDATE games SET play_status = ?1, updated_at = datetime('now','localtime') WHERE id = ?2",
                    params![status, id],
                )?;
            }
            Ok(())
        })
    }

    /// 批量收藏 / 取消收藏
    pub fn set_games_favorite(&self, ids: &[i64], favorite: bool) -> AppResult<()> {
        self.with_tx(|tx| {
            for id in ids {
                tx.execute(
                    "UPDATE games SET favorite = ?1, updated_at = datetime('now','localtime') WHERE id = ?2",
                    params![if favorite { 1 } else { 0 }, id],
                )?;
            }
            Ok(())
        })
    }

    /// 批量给游戏追加标签（不覆盖已有标签）
    pub fn add_tags_to_games(&self, ids: &[i64], tags: &[String]) -> AppResult<()> {
        self.with_tx(|tx| {
            for id in ids {
                for name in tags {
                    let tag_id = Self::ensure_tag(tx, name, None)?;
                    tx.execute(
                        "INSERT OR IGNORE INTO game_tags(game_id, tag_id) VALUES (?1, ?2)",
                        params![id, tag_id],
                    )?;
                }
            }
            Ok(())
        })
    }

    /// 拖动排序后整体落库
    pub fn reorder_games(&self, ordered_ids: &[i64]) -> AppResult<()> {
        self.with_tx(|tx| {
            for (index, id) in ordered_ids.iter().enumerate() {
                tx.execute(
                    "UPDATE games SET sort_order = ?1 WHERE id = ?2",
                    params![index as i64, id],
                )?;
            }
            Ok(())
        })
    }

    /// 累加游玩时长并刷新最近游玩时间
    pub fn accumulate_playtime(&self, game_id: i64, seconds: i64, ended_at: &str) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE games
                 SET total_play_seconds = total_play_seconds + ?1,
                     last_played_at = ?2,
                     updated_at = datetime('now','localtime')
                 WHERE id = ?3",
                params![seconds, ended_at, game_id],
            )?;
            Ok(())
        })
    }

    /// 记录一次游玩会话
    pub fn insert_session(
        &self,
        game_id: i64,
        started_at: &str,
        ended_at: &str,
        duration: i64,
    ) -> AppResult<i64> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO play_sessions(game_id, started_at, ended_at, duration_seconds)
                 VALUES (?1, ?2, ?3, ?4)",
                params![game_id, started_at, ended_at, duration],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn sessions_of_game(&self, game_id: i64, limit: i64) -> AppResult<Vec<PlaySession>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, game_id, started_at, ended_at, duration_seconds, note
                 FROM play_sessions WHERE game_id = ?1
                 ORDER BY started_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![game_id, limit], map_session)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    pub fn delete_session(&self, id: i64) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM play_sessions WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    // ==================== 存档槽位 ====================

    pub fn list_save_slots(&self, game_id: i64) -> AppResult<Vec<SaveSlot>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, game_id, slot_name, engine, source_path, backup_path,
                        size_bytes, file_count, remark, created_at
                 FROM save_slots WHERE game_id = ?1 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![game_id], map_save_slot)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    pub fn insert_save_slot(
        &self,
        game_id: i64,
        slot_name: &str,
        engine: Option<&str>,
        source_path: &str,
        backup_path: &str,
        size_bytes: i64,
        file_count: i64,
        remark: Option<&str>,
    ) -> AppResult<i64> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO save_slots(game_id, slot_name, engine, source_path, backup_path,
                                        size_bytes, file_count, remark)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    game_id,
                    slot_name,
                    engine,
                    source_path,
                    backup_path,
                    size_bytes,
                    file_count,
                    remark
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn get_save_slot(&self, id: i64) -> AppResult<SaveSlot> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, game_id, slot_name, engine, source_path, backup_path,
                        size_bytes, file_count, remark, created_at
                 FROM save_slots WHERE id = ?1",
                params![id],
                map_save_slot,
            )
            .optional()?
            .ok_or_else(|| AppError::msg("存档槽位不存在"))
        })
    }

    pub fn update_save_slot_remark(&self, id: i64, remark: Option<&str>) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE save_slots SET remark = ?1 WHERE id = ?2",
                params![remark, id],
            )?;
            Ok(())
        })
    }

    pub fn delete_save_slot(&self, id: i64) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM save_slots WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    // ==================== 补丁 / 笔记 / 链接 ====================

    pub fn list_patches(&self, game_id: i64) -> AppResult<Vec<Patch>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, game_id, name, version, patch_type, file_path, url, installed, remark, created_at
                 FROM patches WHERE game_id = ?1 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![game_id], |row| {
                Ok(Patch {
                    id: row.get(0)?,
                    game_id: row.get(1)?,
                    name: row.get(2)?,
                    version: row.get(3)?,
                    patch_type: row.get(4)?,
                    file_path: row.get(5)?,
                    url: row.get(6)?,
                    installed: row.get(7)?,
                    remark: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_patch(
        &self,
        id: Option<i64>,
        game_id: i64,
        name: &str,
        version: Option<&str>,
        patch_type: &str,
        file_path: Option<&str>,
        url: Option<&str>,
        installed: bool,
        remark: Option<&str>,
    ) -> AppResult<i64> {
        self.with_conn(|conn| {
            if let Some(id) = id {
                conn.execute(
                    "UPDATE patches SET name=?1, version=?2, patch_type=?3, file_path=?4,
                            url=?5, installed=?6, remark=?7 WHERE id=?8",
                    params![
                        name,
                        version,
                        patch_type,
                        file_path,
                        url,
                        if installed { 1 } else { 0 },
                        remark,
                        id
                    ],
                )?;
                Ok(id)
            } else {
                conn.execute(
                    "INSERT INTO patches(game_id, name, version, patch_type, file_path, url, installed, remark)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![
                        game_id,
                        name,
                        version,
                        patch_type,
                        file_path,
                        url,
                        if installed { 1 } else { 0 },
                        remark
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            }
        })
    }

    pub fn delete_patch(&self, id: i64) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM patches WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn list_notes(&self, game_id: i64) -> AppResult<Vec<Note>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, game_id, title, content, updated_at, created_at
                 FROM notes WHERE game_id = ?1 ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map(params![game_id], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    game_id: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    updated_at: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    pub fn upsert_note(
        &self,
        id: Option<i64>,
        game_id: i64,
        title: &str,
        content: &str,
    ) -> AppResult<i64> {
        self.with_conn(|conn| {
            if let Some(id) = id {
                conn.execute(
                    "UPDATE notes SET title=?1, content=?2, updated_at=datetime('now','localtime') WHERE id=?3",
                    params![title, content, id],
                )?;
                Ok(id)
            } else {
                conn.execute(
                    "INSERT INTO notes(game_id, title, content) VALUES (?1, ?2, ?3)",
                    params![game_id, title, content],
                )?;
                Ok(conn.last_insert_rowid())
            }
        })
    }

    pub fn delete_note(&self, id: i64) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn list_links(&self, game_id: Option<i64>) -> AppResult<Vec<ResourceLink>> {
        self.with_conn(|conn| {
            let sql = match game_id {
                Some(_) => {
                    "SELECT id, game_id, title, url, kind, remark, created_at
                     FROM resource_links WHERE game_id = ?1 ORDER BY created_at DESC"
                }
                None => {
                    "SELECT id, game_id, title, url, kind, remark, created_at
                     FROM resource_links ORDER BY created_at DESC"
                }
            };
            let mut stmt = conn.prepare(sql)?;
            let mapper = |row: &Row| {
                Ok(ResourceLink {
                    id: row.get(0)?,
                    game_id: row.get(1)?,
                    title: row.get(2)?,
                    url: row.get(3)?,
                    kind: row.get(4)?,
                    remark: row.get(5)?,
                    created_at: row.get(6)?,
                })
            };
            let rows = match game_id {
                Some(id) => stmt.query_map(params![id], mapper)?,
                None => stmt.query_map([], mapper)?,
            };
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    pub fn insert_link(
        &self,
        game_id: Option<i64>,
        title: &str,
        url: &str,
        kind: &str,
        remark: Option<&str>,
    ) -> AppResult<i64> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO resource_links(game_id, title, url, kind, remark)
                 VALUES (?1,?2,?3,?4,?5)",
                params![game_id, title, url, kind, remark],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn delete_link(&self, id: i64) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM resource_links WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    // ==================== 统计 ====================

    pub fn overview(&self) -> AppResult<StatsOverview> {
        self.with_conn(|conn| {
            let total_games: i64 =
                conn.query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))?;
            let total_play_seconds: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(total_play_seconds), 0) FROM games",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let count_status = |status: &str| -> i64 {
                conn.query_row(
                    "SELECT COUNT(*) FROM games WHERE play_status = ?1",
                    params![status],
                    |row| row.get(0),
                )
                .unwrap_or(0)
            };
            let favorite_games: i64 = conn
                .query_row("SELECT COUNT(*) FROM games WHERE favorite = 1", [], |row| {
                    row.get(0)
                })
                .unwrap_or(0);
            let range_seconds = |days: i64| -> i64 {
                conn.query_row(
                    "SELECT COALESCE(SUM(duration_seconds), 0) FROM play_sessions
                     WHERE started_at >= datetime('now','localtime', ?1)",
                    params![format!("-{} days", days)],
                    |row| row.get(0),
                )
                .unwrap_or(0)
            };
            let save_slot_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM save_slots", [], |row| row.get(0))
                .unwrap_or(0);

            Ok(StatsOverview {
                total_games,
                total_play_seconds,
                completed_games: count_status(STATUS_COMPLETED),
                playing_games: count_status(STATUS_PLAYING),
                favorite_games,
                week_play_seconds: range_seconds(7),
                month_play_seconds: range_seconds(30),
                save_slot_count,
            })
        })
    }

    /// 每日时长分布，用于热力图 / 折线图
    pub fn daily_playtime(&self, days: i64) -> AppResult<Vec<DailyPlaytime>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT date(started_at) AS d, COALESCE(SUM(duration_seconds), 0), COUNT(*)
                 FROM play_sessions
                 WHERE started_at >= datetime('now','localtime', ?1)
                 GROUP BY d ORDER BY d ASC",
            )?;
            let rows = stmt.query_map(params![format!("-{} days", days)], |row| {
                Ok(DailyPlaytime {
                    date: row.get(0)?,
                    seconds: row.get(1)?,
                    sessions: row.get(2)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// 游戏时长排行
    pub fn playtime_ranking(&self, limit: i64, year: Option<i32>) -> AppResult<Vec<GamePlaytimeRank>> {
        self.with_conn(|conn| {
            let (sql, args): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match year {
                Some(y) => (
                    "SELECT g.id, g.title, g.cover_path,
                            COALESCE(SUM(ps.duration_seconds), 0) AS secs, COUNT(ps.id)
                     FROM games g
                     JOIN play_sessions ps ON ps.game_id = g.id
                     WHERE strftime('%Y', ps.started_at) = ?
                     GROUP BY g.id ORDER BY secs DESC LIMIT ?",
                    vec![Box::new(y.to_string()), Box::new(limit)],
                ),
                None => (
                    "SELECT g.id, g.title, g.cover_path, g.total_play_seconds,
                            (SELECT COUNT(*) FROM play_sessions ps WHERE ps.game_id = g.id)
                     FROM games g WHERE g.total_play_seconds > 0
                     ORDER BY g.total_play_seconds DESC LIMIT ?",
                    vec![Box::new(limit)],
                ),
            };
            let mut stmt = conn.prepare(sql)?;
            let params_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
            let rows = stmt.query_map(params_ref.as_slice(), |row| {
                Ok(GamePlaytimeRank {
                    game_id: row.get(0)?,
                    title: row.get(1)?,
                    cover_path: row.get(2)?,
                    seconds: row.get(3)?,
                    sessions: row.get(4)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// 年度游玩报告
    pub fn yearly_report(&self, year: i32) -> AppResult<YearlyReport> {
        self.with_conn(|conn| {
            let year_str = year.to_string();
            let (total_seconds, total_sessions): (i64, i64) = conn.query_row(
                "SELECT COALESCE(SUM(duration_seconds),0), COUNT(*) FROM play_sessions
                 WHERE strftime('%Y', started_at) = ?1",
                params![year_str],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;

            let active_days: i64 = conn.query_row(
                "SELECT COUNT(DISTINCT date(started_at)) FROM play_sessions
                 WHERE strftime('%Y', started_at) = ?1",
                params![year_str],
                |row| row.get(0),
            )?;

            let (longest_seconds, longest_game): (i64, Option<String>) = conn
                .query_row(
                    "SELECT COALESCE(MAX(ps.duration_seconds), 0),
                            (SELECT g.title FROM games g WHERE g.id = ps.game_id)
                     FROM play_sessions ps WHERE strftime('%Y', ps.started_at) = ?1",
                    params![year_str],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap_or((0, None));

            let completed_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM games WHERE play_status = 'completed'
                       AND strftime('%Y', updated_at) = ?1",
                    params![year_str],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let added_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM games WHERE strftime('%Y', created_at) = ?1",
                    params![year_str],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let mut monthly = vec![0i64; 12];
            {
                let mut stmt = conn.prepare(
                    "SELECT CAST(strftime('%m', started_at) AS INTEGER), COALESCE(SUM(duration_seconds),0)
                     FROM play_sessions WHERE strftime('%Y', started_at) = ?1
                     GROUP BY 1",
                )?;
                let rows = stmt.query_map(params![year_str], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })?;
                for row in rows {
                    let (month, seconds) = row?;
                    if (1..=12).contains(&month) {
                        monthly[(month - 1) as usize] = seconds;
                    }
                }
            }

            let mut weekday = vec![0i64; 7];
            {
                // SQLite 的 %w：0=周日。转换为 0=周一 更符合中文习惯。
                let mut stmt = conn.prepare(
                    "SELECT CAST(strftime('%w', started_at) AS INTEGER), COALESCE(SUM(duration_seconds),0)
                     FROM play_sessions WHERE strftime('%Y', started_at) = ?1
                     GROUP BY 1",
                )?;
                let rows = stmt.query_map(params![year_str], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })?;
                for row in rows {
                    let (dow, seconds) = row?;
                    let index = if dow == 0 { 6 } else { (dow - 1) as usize };
                    if index < 7 {
                        weekday[index] = seconds;
                    }
                }
            }

            let daily = {
                let mut stmt = conn.prepare(
                    "SELECT date(started_at), COALESCE(SUM(duration_seconds),0), COUNT(*)
                     FROM play_sessions WHERE strftime('%Y', started_at) = ?1
                     GROUP BY 1 ORDER BY 1",
                )?;
                let rows = stmt.query_map(params![year_str], |row| {
                    Ok(DailyPlaytime {
                        date: row.get(0)?,
                        seconds: row.get(1)?,
                        sessions: row.get(2)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };

            let ranking = {
                let mut stmt = conn.prepare(
                    "SELECT g.id, g.title, g.cover_path,
                            COALESCE(SUM(ps.duration_seconds),0), COUNT(ps.id)
                     FROM games g JOIN play_sessions ps ON ps.game_id = g.id
                     WHERE strftime('%Y', ps.started_at) = ?1
                     GROUP BY g.id ORDER BY 4 DESC LIMIT 20",
                )?;
                let rows = stmt.query_map(params![year_str], |row| {
                    Ok(GamePlaytimeRank {
                        game_id: row.get(0)?,
                        title: row.get(1)?,
                        cover_path: row.get(2)?,
                        seconds: row.get(3)?,
                        sessions: row.get(4)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };

            Ok(YearlyReport {
                year,
                total_seconds,
                total_sessions,
                active_days,
                average_seconds: if active_days > 0 {
                    total_seconds / active_days
                } else {
                    0
                },
                longest_session_seconds: longest_seconds,
                longest_session_game: longest_game,
                top_game: ranking.first().map(|r| r.title.clone()),
                completed_count,
                added_count,
                monthly,
                weekday,
                daily,
                ranking,
            })
        })
    }

    /// 有游玩记录的年份列表（供报告页选择）
    pub fn play_years(&self) -> AppResult<Vec<i32>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT CAST(strftime('%Y', started_at) AS INTEGER) AS y
                 FROM play_sessions ORDER BY y DESC",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, i32>(0))?;
            let mut years = rows.collect::<Result<Vec<_>, _>>()?;
            let current = chrono::Local::now().year();
            if !years.contains(&current) {
                years.insert(0, current);
            }
            Ok(years)
        })
    }

    /// 引擎分布统计
    pub fn engine_distribution(&self) -> AppResult<Vec<(String, i64)>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT IFNULL(engine, 'unknown'), COUNT(*) FROM games GROUP BY 1 ORDER BY 2 DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// 时间线：最近的游玩会话（跨游戏）
    pub fn recent_sessions(&self, limit: i64) -> AppResult<Vec<PlaySession>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT ps.id, ps.game_id, ps.started_at, ps.ended_at, ps.duration_seconds, ps.note,
                        g.title
                 FROM play_sessions ps JOIN games g ON g.id = ps.game_id
                 ORDER BY ps.started_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |row| {
                Ok(PlaySession {
                    id: row.get(0)?,
                    game_id: row.get(1)?,
                    started_at: row.get(2)?,
                    ended_at: row.get(3)?,
                    duration_seconds: row.get(4)?,
                    note: row.get(5)?,
                    game_title: row.get(6)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }
}

use chrono::Datelike;

// ==================== 行映射 ====================

fn map_category(row: &Row) -> rusqlite::Result<Category> {
    Ok(Category {
        id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        sort_order: row.get(3)?,
        game_count: row.get(4)?,
    })
}

fn map_tag(row: &Row) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        game_count: row.get(3)?,
    })
}

fn map_game(row: &Row) -> rusqlite::Result<Game> {
    Ok(Game {
        id: row.get("id")?,
        title: row.get("title")?,
        original_title: row.get("original_title")?,
        path: row.get("path")?,
        executable: row.get("executable")?,
        args: row.get("args")?,
        cover_path: row.get("cover_path")?,
        engine: row.get("engine")?,
        engine_confidence: row.get("engine_confidence")?,
        category_id: row.get("category_id")?,
        play_status: row.get("play_status")?,
        favorite: row.get("favorite")?,
        rating: row.get("rating")?,
        le_launch: row.get("le_launch")?,
        le_locale: row.get("le_locale")?,
        total_play_seconds: row.get("total_play_seconds")?,
        last_played_at: row.get("last_played_at")?,
        release_date: row.get("release_date")?,
        developer: row.get("developer")?,
        description: row.get("description")?,
        notes: row.get("notes")?,
        save_path: row.get("save_path")?,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        tags: Vec::new(),
        session_count: row.get("session_count")?,
        save_count: row.get("save_count")?,
    })
}

fn map_session(row: &Row) -> rusqlite::Result<PlaySession> {
    Ok(PlaySession {
        id: row.get(0)?,
        game_id: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        duration_seconds: row.get(4)?,
        note: row.get(5)?,
        game_title: None,
    })
}

fn map_save_slot(row: &Row) -> rusqlite::Result<SaveSlot> {
    Ok(SaveSlot {
        id: row.get(0)?,
        game_id: row.get(1)?,
        slot_name: row.get(2)?,
        engine: row.get(3)?,
        source_path: row.get(4)?,
        backup_path: row.get(5)?,
        size_bytes: row.get(6)?,
        file_count: row.get(7)?,
        remark: row.get(8)?,
        created_at: row.get(9)?,
    })
}

// ==================== Schema ====================

const SCHEMA_VERSION: i32 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS categories (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE,
    icon       TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tags (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL UNIQUE,
    color TEXT
);

CREATE TABLE IF NOT EXISTS games (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    title             TEXT NOT NULL,
    original_title    TEXT,
    path              TEXT,
    executable        TEXT,
    args              TEXT,
    cover_path        TEXT,
    engine            TEXT,
    engine_confidence INTEGER NOT NULL DEFAULT 0,
    category_id       INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    play_status       TEXT NOT NULL DEFAULT 'unplayed',
    favorite          INTEGER NOT NULL DEFAULT 0,
    rating            INTEGER NOT NULL DEFAULT -1,
    le_launch         INTEGER NOT NULL DEFAULT 0,
    le_locale         TEXT DEFAULT 'ja_JP',
    total_play_seconds INTEGER NOT NULL DEFAULT 0,
    last_played_at    TEXT,
    release_date      TEXT,
    developer         TEXT,
    description       TEXT,
    notes             TEXT,
    save_path         TEXT,
    sort_order        INTEGER NOT NULL DEFAULT 0,
    -- 拼音检索索引（全拼 + 首字母），由 search::build_search_index 生成
    search_index      TEXT,
    created_at        TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE INDEX IF NOT EXISTS idx_games_category ON games(category_id);
CREATE INDEX IF NOT EXISTS idx_games_status   ON games(play_status);
CREATE INDEX IF NOT EXISTS idx_games_played   ON games(last_played_at DESC);
CREATE INDEX IF NOT EXISTS idx_games_path     ON games(path);

CREATE TABLE IF NOT EXISTS game_tags (
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
    PRIMARY KEY (game_id, tag_id)
);

CREATE TABLE IF NOT EXISTS play_sessions (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id          INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    started_at       TEXT NOT NULL,
    ended_at         TEXT,
    duration_seconds INTEGER NOT NULL DEFAULT 0,
    note             TEXT
);

CREATE INDEX IF NOT EXISTS idx_sessions_game  ON play_sessions(game_id);
CREATE INDEX IF NOT EXISTS idx_sessions_start ON play_sessions(started_at DESC);

CREATE TABLE IF NOT EXISTS save_slots (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id     INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    slot_name   TEXT NOT NULL,
    engine      TEXT,
    source_path TEXT NOT NULL,
    backup_path TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL DEFAULT 0,
    file_count  INTEGER NOT NULL DEFAULT 0,
    remark      TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE INDEX IF NOT EXISTS idx_slots_game ON save_slots(game_id);

CREATE TABLE IF NOT EXISTS patches (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id    INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    version    TEXT,
    patch_type TEXT NOT NULL DEFAULT 'translation',
    file_path  TEXT,
    url        TEXT,
    installed  INTEGER NOT NULL DEFAULT 0,
    remark     TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE INDEX IF NOT EXISTS idx_patches_game ON patches(game_id);

CREATE TABLE IF NOT EXISTS notes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id    INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    content    TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE INDEX IF NOT EXISTS idx_notes_game ON notes(game_id);

CREATE TABLE IF NOT EXISTS resource_links (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id    INTEGER REFERENCES games(id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    url        TEXT NOT NULL,
    kind       TEXT NOT NULL DEFAULT 'other',
    remark     TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 每个用例用独立的临时库文件，避免互相干扰
    fn temp_db(tag: &str) -> (Db, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "galmanager_dbtest_{tag}_{}.db",
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

    fn add_game(db: &Db, title: &str, original: Option<&str>, developer: Option<&str>) -> i64 {
        db.create_game(&GameInput {
            title: title.to_string(),
            original_title: original.map(str::to_string),
            developer: developer.map(str::to_string),
            ..Default::default()
        })
        .expect("创建游戏失败")
    }

    fn search(db: &Db, keyword: &str) -> Vec<String> {
        let filter = GameFilter {
            keyword: keyword.to_string(),
            ..Default::default()
        };
        let mut titles: Vec<String> = db
            .list_games(&filter)
            .expect("查询失败")
            .into_iter()
            .map(|game| game.title)
            .collect();
        titles.sort();
        titles
    }

    #[test]
    fn pinyin_initials_and_full_pinyin_find_chinese_title() {
        let (db, path) = temp_db("pinyin_basic");
        add_game(&db, "千恋万花", Some("Senren * Banka"), Some("柚子社"));
        add_game(&db, "白色相簿2", None, Some("Leaf"));

        assert_eq!(search(&db, "qlwh"), ["千恋万花"], "首字母检索失败");
        assert_eq!(search(&db, "qianlian"), ["千恋万花"], "全拼检索失败");
        assert_eq!(search(&db, "千恋"), ["千恋万花"], "原文检索应仍然可用");
        assert_eq!(search(&db, "QLWH"), ["千恋万花"], "关键词应先转小写");
        cleanup(&path);
    }

    #[test]
    fn pinyin_search_matches_developer() {
        let (db, path) = temp_db("pinyin_dev");
        add_game(&db, "千恋万花", None, Some("柚子社"));
        add_game(&db, "CLANNAD", None, Some("Key"));

        assert_eq!(search(&db, "yzs"), ["千恋万花"]);
        assert_eq!(search(&db, "youzishe"), ["千恋万花"]);
        cleanup(&path);
    }

    #[test]
    fn pinyin_search_does_not_over_match() {
        let (db, path) = temp_db("pinyin_negative");
        add_game(&db, "白色相簿2", None, None);
        assert!(search(&db, "qlwh").is_empty(), "不该命中无关标题");
        cleanup(&path);
    }

    #[test]
    fn search_index_is_refreshed_after_update() {
        let (db, path) = temp_db("pinyin_update");
        let id = add_game(&db, "白色相簿2", None, None);
        assert!(search(&db, "qlwh").is_empty());

        db.update_game(
            id,
            &GameInput {
                title: "千恋万花".to_string(),
                ..Default::default()
            },
        )
        .expect("更新失败");

        assert_eq!(search(&db, "qlwh"), ["千恋万花"], "改名后索引应同步刷新");
        cleanup(&path);
    }

    /// 老库（没有 search_index 列）打开时应自动补列并回填，升级后搜索立即可用
    #[test]
    fn legacy_database_is_migrated_and_backfilled() {
        let path = std::env::temp_dir().join(format!(
            "galmanager_dbtest_legacy_{}.db",
            std::process::id()
        ));
        cleanup(&path);

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE games (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    title TEXT NOT NULL,
                    original_title TEXT, path TEXT, executable TEXT, args TEXT, cover_path TEXT,
                    engine TEXT, engine_confidence INTEGER NOT NULL DEFAULT 0, category_id INTEGER,
                    play_status TEXT NOT NULL DEFAULT 'unplayed', favorite INTEGER NOT NULL DEFAULT 0,
                    rating INTEGER NOT NULL DEFAULT -1, le_launch INTEGER NOT NULL DEFAULT 0,
                    le_locale TEXT, total_play_seconds INTEGER NOT NULL DEFAULT 0, last_played_at TEXT,
                    release_date TEXT, developer TEXT, description TEXT, notes TEXT, save_path TEXT,
                    sort_order INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
                 );
                 INSERT INTO games(title, developer) VALUES ('千恋万花', '柚子社');",
            )
            .unwrap();
        }

        let db = Db::open(&path).expect("打开老库失败");
        assert_eq!(
            search(&db, "qlwh"),
            ["千恋万花"],
            "老库升级后应能立即用拼音搜到"
        );
        cleanup(&path);
    }
}
