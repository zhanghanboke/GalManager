//! 游戏启动与游玩时长监控。
//!
//! 两件事：
//! 1. 组装启动命令（可经 Locale Emulator 转区，可回退提权启动）；
//! 2. 后台轮询进程存活状态，进程退出时结算本次游玩时长并落库。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::Game;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Locale Emulator 默认启动参数模板。
///
/// `{exe}` 会被替换为游戏可执行文件绝对路径。直接经由 `LEProc.exe` 启动即会
/// 应用 LE 中配置的默认区域（通常为日语），因此默认模板即可实现「一键转区」。
/// 需要显式指定区域档案时，可改为 `-runas {locale} {exe}`。
pub const DEFAULT_LE_TEMPLATE: &str = "{exe}";

/// 一次运行中的游戏会话
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningGame {
    pub game_id: i64,
    pub title: String,
    /// 被监控的进程名（小写，含 .exe）
    pub process_name: String,
    pub pid: u32,
    pub started_at: String,
    pub via_locale_emulator: bool,
}

/// 启动结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOutcome {
    pub ok: bool,
    pub message: String,
    pub pid: Option<u32>,
    pub via_locale_emulator: bool,
    /// 进程已被监控（用于前端提示「开始计时」）
    pub tracking: bool,
}

/// 全局运行态：记录正在游玩中的游戏
#[derive(Default, Clone)]
pub struct LauncherState {
    inner: Arc<Mutex<HashMap<i64, RunningGame>>>,
}

impl LauncherState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn running(&self) -> Vec<RunningGame> {
        self.inner.lock().values().cloned().collect()
    }

    pub fn get(&self, game_id: i64) -> Option<RunningGame> {
        self.inner.lock().get(&game_id).cloned()
    }

    fn insert(&self, session: RunningGame) {
        self.inner.lock().insert(session.game_id, session);
    }

    fn remove(&self, game_id: i64) -> Option<RunningGame> {
        self.inner.lock().remove(&game_id)
    }
}

/// 自动探测 Locale Emulator 安装位置
pub fn detect_locale_emulator() -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(program_files) = std::env::var("ProgramFiles") {
        candidates.push(PathBuf::from(&program_files).join("Locale Emulator").join("LEProc.exe"));
        candidates.push(PathBuf::from(&program_files).join("LocaleEmulator").join("LEProc.exe"));
    }
    if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
        candidates.push(
            PathBuf::from(&program_files_x86)
                .join("Locale Emulator")
                .join("LEProc.exe"),
        );
    }
    for drive in ["C", "D", "E"] {
        candidates.push(PathBuf::from(format!("{}:\\Locale Emulator\\LEProc.exe", drive)));
        candidates.push(PathBuf::from(format!("{}:\\Tools\\Locale Emulator\\LEProc.exe", drive)));
        candidates.push(PathBuf::from(format!("{}:\\Program Files\\Locale Emulator\\LEProc.exe", drive)));
    }

    for path in candidates {
        if path.is_file() {
            log::info!("自动探测到 Locale Emulator: {}", path.display());
            return Some(path.to_string_lossy().to_string());
        }
    }
    None
}

/// 解析游戏的启动目标
fn resolve_executable(game: &Game) -> AppResult<(PathBuf, PathBuf)> {
    let game_dir = game
        .path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::msg("该游戏未设置安装目录"))?;
    if !game_dir.is_dir() {
        return Err(AppError::msg(format!(
            "游戏目录不存在: {}",
            game_dir.display()
        )));
    }

    let exe_relative = game
        .executable
        .as_ref()
        .filter(|e| !e.trim().is_empty())
        .ok_or_else(|| AppError::msg("该游戏未设置启动程序，请在详情页指定"))?;

    let exe_path = game_dir.join(exe_relative);
    if !exe_path.is_file() {
        // 容错：若设置的路径失效，尝试在根目录找一个可用的启动项
        let fallback = crate::scanner::list_executables(&game_dir)?
            .into_iter()
            .next()
            .map(|name| game_dir.join(name));
        return match fallback {
            Some(path) => {
                log::warn!(
                    "启动程序 {} 不存在，回退到 {}",
                    exe_path.display(),
                    path.display()
                );
                Ok((game_dir, path))
            }
            None => Err(AppError::msg(format!(
                "启动程序不存在: {}",
                exe_path.display()
            ))),
        };
    }

    Ok((game_dir, exe_path))
}

/// 按模板拼装参数列表
fn render_template(template: &str, exe: &str, args: Option<&str>, locale: &str, dir: &str) -> Vec<String> {
    let expanded = template
        .replace("{exe}", exe)
        .replace("{locale}", locale)
        .replace("{dir}", dir)
        .replace("{args}", args.unwrap_or(""));

    // 支持模板中出现带引号的路径；简单按空白切分后再去掉包裹引号
    expanded
        .split_whitespace()
        .map(|token| token.trim_matches('"').to_string())
        .filter(|token| !token.is_empty())
        .collect()
}

/// 启动游戏
pub fn launch(
    app: &AppHandle,
    db: &Db,
    launcher: &LauncherState,
    game: &Game,
) -> AppResult<LaunchOutcome> {
    if launcher.get(game.id).is_some() {
        return Ok(LaunchOutcome {
            ok: false,
            message: format!("《{}》已在运行中", game.title),
            pid: None,
            via_locale_emulator: game.le_launch == 1,
            tracking: true,
        });
    }

    let (game_dir, exe_path) = resolve_executable(game)?;
    let exe_str = exe_path.to_string_lossy().to_string();
    let dir_str = game_dir.to_string_lossy().to_string();
    let process_name = exe_path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let use_le = game.le_launch == 1;
    let le_path = if use_le {
        let configured = db.get_setting("le_path")?.filter(|p| !p.trim().is_empty());
        let resolved = configured.or_else(detect_locale_emulator);
        match resolved {
            Some(path) => {
                let path_buf = PathBuf::from(&path);
                if !path_buf.is_file() {
                    return Err(AppError::msg(format!(
                        "Locale Emulator 路径无效: {}，请在设置中重新指定",
                        path
                    )));
                }
                Some(path)
            }
            None => {
                return Err(AppError::msg(
                    "未检测到 Locale Emulator，请在「设置 → 启动」中指定 LEProc.exe 路径，或关闭该游戏的转区启动",
                ))
            }
        }
    } else {
        None
    };

    let le_template = db
        .get_setting("le_args_template")?
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_LE_TEMPLATE.to_string());
    let le_locale = game
        .le_locale
        .clone()
        .unwrap_or_else(|| "ja-JP".to_string());

    let mut command = if let Some(le) = &le_path {
        let mut cmd = Command::new(le);
        cmd.current_dir(&game_dir);
        cmd.args(render_template(
            &le_template,
            &exe_str,
            game.args.as_deref(),
            &le_locale,
            &dir_str,
        ));
        cmd
    } else {
        let mut cmd = Command::new(&exe_path);
        cmd.current_dir(&game_dir);
        if let Some(args) = game.args.as_deref().filter(|a| !a.trim().is_empty()) {
            cmd.args(args.split_whitespace());
        }
        cmd
    };

    command.stdin(Stdio::null());

    log::info!(
        "启动游戏 game_id={} title={} le={} exe={} cwd={}",
        game.id,
        game.title,
        use_le,
        exe_str,
        dir_str
    );

    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(AppError::msg(format!(
                "启动失败: {error}（可执行文件: {exe_str}）"
            )))
        }
    };

    let pid = child.id();
    let started_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let session = RunningGame {
        game_id: game.id,
        title: game.title.clone(),
        process_name: process_name.clone(),
        pid,
        started_at: started_at.clone(),
        via_locale_emulator: use_le,
    };
    launcher.insert(session.clone());

    // 落一条未结束的会话记录，异常退出也能在时间线上看到
    if let Err(error) = db.insert_session(game.id, &started_at, &started_at, 0) {
        log::warn!("写入游玩会话失败 game_id={}: {}", game.id, error);
    }

    // 游戏启动后自动切到「在玩」状态
    if game.play_status == crate::models::STATUS_UNPLAYED {
        let _ = db.set_games_status(&[game.id], crate::models::STATUS_PLAYING);
    }

    let _ = app.emit("game-launched", &session);

    // 启动后台监控
    spawn_monitor(app.clone(), db.clone(), launcher.clone(), session.clone());

    Ok(LaunchOutcome {
        ok: true,
        message: if use_le {
            format!("已通过 Locale Emulator 转区启动《{}》", game.title)
        } else {
            format!("已启动《{}》", game.title)
        },
        pid: Some(pid),
        via_locale_emulator: use_le,
        tracking: true,
    })
}

/// 轮询进程是否仍存活，退出时结算时长。
///
/// 采用 `sysinfo` 按进程名匹配而非 PID：经 LE 启动时，LEProc 会再拉起真正的
/// 游戏进程，PID 与父进程并不一致；部分游戏还会自我重启，按名字匹配更稳。
fn spawn_monitor(
    app: AppHandle,
    db: Db,
    launcher: LauncherState,
    session: RunningGame,
) {
    std::thread::spawn(move || {
        use sysinfo::{ProcessesToUpdate, System};

        const POLL_INTERVAL_MS: u64 = 3000;
        // 启动初期给一点缓冲，避免游戏还没拉起来就被判定为已退出
        const GRACE_PERIOD_MS: u64 = 20_000;
        // 连续多次探测不到才认为真的退出了（应对加载卡顿 / 进程短暂消失）
        const MISS_THRESHOLD: u32 = 3;

        let started = std::time::Instant::now();
        let mut miss_count = 0u32;
        let mut seen_once = false;

        let mut system = System::new();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));

            // 会话已被手动停止（stop_game 会移除记录）
            if launcher.get(session.game_id).is_none() {
                return;
            }

            system.refresh_processes(ProcessesToUpdate::All, true);
            let found = system.processes().values().any(|process| {
                process
                    .name()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&session.process_name)
            });

            if found {
                seen_once = true;
                miss_count = 0;
                continue;
            }

            if !seen_once && started.elapsed().as_millis() < GRACE_PERIOD_MS as u128 {
                continue; // 仍在启动缓冲期内
            }

            miss_count += 1;
            if miss_count < MISS_THRESHOLD {
                continue;
            }

            // ---- 结算 ----
            let ended_at = chrono::Local::now();
            let ended_str = ended_at.format("%Y-%m-%d %H:%M:%S").to_string();
            // 起止时间都是本地时间字符串，直接按 NaiveDateTime 求差即可
            let duration = chrono::NaiveDateTime::parse_from_str(
                &session.started_at,
                "%Y-%m-%d %H:%M:%S",
            )
            .map(|started| (ended_at.naive_local() - started).num_seconds().max(0))
            .unwrap_or(0);

            if let Err(error) = finalize_session(&db, &session, &ended_str, duration) {
                log::error!("结算游玩时长失败 game_id={}: {}", session.game_id, error);
            }
            launcher.remove(session.game_id);
            let _ = app.emit("game-exited", (session.game_id, duration));
            log::info!(
                "游戏退出 game_id={} title={} 本次时长={}秒",
                session.game_id,
                session.title,
                duration
            );
            return;
        }
    });
}

fn finalize_session(
    db: &Db,
    session: &RunningGame,
    ended_at: &str,
    duration: i64,
) -> AppResult<()> {
    // 更新那条占位会话；找不到就补一条
    let updated = db.with_conn(|conn| {
        let affected = conn.execute(
            "UPDATE play_sessions
             SET ended_at = ?1, duration_seconds = ?2
             WHERE id = (SELECT id FROM play_sessions
                         WHERE game_id = ?3 AND ended_at = started_at
                         ORDER BY id DESC LIMIT 1)",
            rusqlite::params![ended_at, duration, session.game_id],
        )?;
        Ok(affected)
    })?;

    if updated == 0 {
        db.insert_session(session.game_id, &session.started_at, ended_at, duration)?;
    }
    db.accumulate_playtime(session.game_id, duration, ended_at)?;
    Ok(())
}

/// 手动停止游戏（按进程名结束）
pub fn stop(launcher: &LauncherState, game_id: i64) -> AppResult<u32> {
    let session = launcher
        .remove(game_id)
        .ok_or_else(|| AppError::msg("该游戏当前不在运行中"))?;

    let mut terminated = 0u32;
    if cfg!(windows) {
        let output = Command::new("taskkill")
            .args(["/F", "/IM", &session.process_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output();
        if output.map(|o| o.status.success()).unwrap_or(false) {
            terminated = 1;
        }
    } else {
        let _ = Command::new("pkill")
            .args(["-f", &session.process_name])
            .status();
        terminated = 1;
    }

    log::info!(
        "手动停止游戏 game_id={} process={} terminated={}",
        game_id,
        session.process_name,
        terminated
    );
    Ok(terminated)
}

/// 打开目录（资源管理器）
pub fn open_in_explorer(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::msg(format!("路径不存在: {}", path.display())));
    }
    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| AppError::msg(format!("打开资源管理器失败: {e}")))?;
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(path).spawn();
    }
    Ok(())
}
