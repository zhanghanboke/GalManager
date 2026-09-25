//! 数据统计命令。

use crate::db::Db;
use crate::error::AppResult;
use crate::models::*;
use tauri::State;

#[tauri::command]
pub fn stats_overview(db: State<'_, Db>) -> AppResult<StatsOverview> {
    db.overview()
}

#[tauri::command]
pub fn stats_daily(db: State<'_, Db>, days: Option<i64>) -> AppResult<Vec<DailyPlaytime>> {
    db.daily_playtime(days.unwrap_or(30))
}

#[tauri::command]
pub fn stats_ranking(db: State<'_, Db>, limit: Option<i64>) -> AppResult<Vec<GamePlaytimeRank>> {
    db.playtime_ranking(limit.unwrap_or(10), None)
}

#[tauri::command]
pub fn stats_yearly_report(db: State<'_, Db>, year: i32) -> AppResult<YearlyReport> {
    db.yearly_report(year)
}

#[tauri::command]
pub fn stats_play_years(db: State<'_, Db>) -> AppResult<Vec<i32>> {
    db.play_years()
}

#[tauri::command]
pub fn stats_engine_distribution(db: State<'_, Db>) -> AppResult<Vec<(String, i64)>> {
    db.engine_distribution()
}

#[tauri::command]
pub fn stats_recent_sessions(db: State<'_, Db>, limit: Option<i64>) -> AppResult<Vec<PlaySession>> {
    db.recent_sessions(limit.unwrap_or(20))
}

#[tauri::command]
pub fn list_game_sessions(
    db: State<'_, Db>,
    game_id: i64,
    limit: Option<i64>,
) -> AppResult<Vec<PlaySession>> {
    db.sessions_of_game(game_id, limit.unwrap_or(50))
}

#[tauri::command]
pub fn delete_session(db: State<'_, Db>, session_id: i64) -> AppResult<()> {
    db.delete_session(session_id)
}

/// 手动补录一次游玩记录（用于迁移旧数据或补记）
#[tauri::command]
pub fn add_session(
    db: State<'_, Db>,
    game_id: i64,
    started_at: String,
    duration_minutes: i64,
) -> AppResult<i64> {
    let duration = duration_minutes.max(0) * 60;
    let ended_at = chrono::NaiveDateTime::parse_from_str(&started_at, "%Y-%m-%d %H:%M:%S")
        .map(|naive| (naive + chrono::Duration::seconds(duration)).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|_| started_at.clone());
    let id = db.insert_session(game_id, &started_at, &ended_at, duration)?;
    db.accumulate_playtime(game_id, duration, &ended_at)?;
    Ok(id)
}
