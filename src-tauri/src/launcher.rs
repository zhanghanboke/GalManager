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

/// 归一化路径用于比较：去掉 Windows 扩展长度前缀、统一分隔符、转小写。
fn normalize_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let stripped = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    stripped.replace('/', "\\").to_lowercase()
}

/// 按空白切分命令行，但**双引号内的空白不切分**，引号本身不保留。
///
/// 用于切分模板本身以及 `{args}` 的取值，例如：
/// `-runas ja-JP "D:/My Games/Game.exe"` → `["-runas", "ja-JP", "D:/My Games/Game.exe"]`
fn split_args(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// 按模板拼装参数列表。
///
/// 关键点：**先切分模板、再做占位符替换**，这样 `{exe}` / `{dir}` 即使取值里含空格
/// 也会作为**单个**参数传给目标程序。若反过来先替换再切分，`D:/My Games/Game.exe`
/// 会被拆成 `D:/My` 和 `Games/Game.exe`，导致 Locale Emulator 启动失败。
///
/// `{args}` 是唯一的例外：它本身代表「多个参数」，因此会按引号感知规则再切分一次。
fn render_template(template: &str, exe: &str, args: Option<&str>, locale: &str, dir: &str) -> Vec<String> {
    let mut out = Vec::new();

    for token in split_args(template) {
        if token == "{args}" {
            // 用户填写的附加参数按空白拆开（引号内不拆）
            out.extend(split_args(args.unwrap_or("")));
            continue;
        }
        out.push(
            token
                .replace("{exe}", exe)
                .replace("{locale}", locale)
                .replace("{dir}", dir)
                .replace("{args}", args.unwrap_or("")),
        );
    }

    out
}

/// Windows 的 `ERROR_ELEVATION_REQUIRED`：目标程序要求以管理员身份运行。
/// 很多 Galgame（尤其是带 DRM / 需要写入安装目录的）以及 Locale Emulator 本身都会触发。
const ERROR_ELEVATION_REQUIRED: i32 = 740;

/// 以管理员身份重新启动（触发 UAC 提权）。
///
/// `std::process::Command` 无法提权，因此改用 `ShellExecuteW` 的 `runas` 谓词。
/// 注意：提权后由系统创建进程，**拿不到子进程句柄与 PID**，
/// 所以游玩时长监控继续依赖进程名轮询（见 `spawn_monitor`）。
#[cfg(windows)]
fn launch_elevated(program: &Path, args: &[String], dir: &Path) -> AppResult<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    // 参数按 Windows 命令行规则重新拼成一个字符串（含空格的参数需要加引号）
    let params: String = args
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let verb = wide(OsStr::new("runas"));
    let file = wide(program.as_os_str());
    let params_w = wide(OsStr::new(&params));
    let dir_w = wide(dir.as_os_str());

    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            if params.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(params_w.as_ptr())
            },
            PCWSTR(dir_w.as_ptr()),
            SW_SHOWNORMAL,
        )
    };

    // ShellExecuteW 返回的 HINSTANCE <= 32 表示失败
    let code = result.0 as isize;
    if code <= 32 {
        let hint = match code {
            5 => "，用户可能取消了 UAC 提权",
            2 => "，找不到可执行文件",
            3 => "，找不到路径",
            _ => "",
        };
        return Err(AppError::msg(format!(
            "提权启动失败（ShellExecuteW 返回 {code}{hint}）：{}",
            program.display()
        )));
    }

    log::info!(
        "已通过 UAC 提权启动: {} {} (cwd={})",
        program.display(),
        params,
        dir.display()
    );
    Ok(())
}

/// 非 Windows 平台没有 UAC，占位实现。
#[cfg(not(windows))]
fn launch_elevated(program: &Path, _args: &[String], _dir: &Path) -> AppResult<()> {
    Err(AppError::msg(format!(
        "当前平台不支持提权启动: {}",
        program.display()
    )))
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

    // 统一算出「要启动谁 / 传什么参数 / 工作目录」，便于 spawn 失败时按同样参数提权重试
    let (program, launch_args): (PathBuf, Vec<String>) = if let Some(le) = &le_path {
        (
            PathBuf::from(le),
            render_template(
                &le_template,
                &exe_str,
                game.args.as_deref(),
                &le_locale,
                &dir_str,
            ),
        )
    } else {
        let args = game
            .args
            .as_deref()
            .filter(|a| !a.trim().is_empty())
            .map(split_args)
            .unwrap_or_default();
        (exe_path.clone(), args)
    };

    let mut command = Command::new(&program);
    command.current_dir(&game_dir);
    command.args(&launch_args);
    command.stdin(Stdio::null());

    log::info!(
        "启动游戏 game_id={} title={} le={} program={} args={:?} cwd={}",
        game.id,
        game.title,
        use_le,
        program.display(),
        launch_args,
        dir_str
    );

    // spawn 成功则拿到子进程；返回 740 说明目标要求管理员权限，回退到 UAC 提权启动
    let mut elevated = false;
    let child = match command.spawn() {
        Ok(child) => Some(child),
        Err(error) if error.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) => {
            log::warn!(
                "直接启动被拒绝（os error 740，需要管理员权限），尝试 UAC 提权: {}",
                program.display()
            );
            launch_elevated(&program, &launch_args, &game_dir)?;
            elevated = true;
            None
        }
        Err(error) => {
            return Err(AppError::msg(format!(
                "启动失败: {error}（可执行文件: {}）",
                program.display()
            )))
        }
    };

    // 提权启动由系统创建进程，拿不到 PID，用 0 表示未知（监控仍按进程名工作）
    let pid = child.as_ref().map(|c| c.id()).unwrap_or(0);
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

    // 启动后台监控（传入预期 exe 路径与游戏目录，用于排除同名进程的误判）
    spawn_monitor(
        app.clone(),
        db.clone(),
        launcher.clone(),
        session.clone(),
        exe_path.clone(),
        game_dir.clone(),
    );

    Ok(LaunchOutcome {
        ok: true,
        message: match (use_le, elevated) {
            (true, true) => format!("已提权并通过 Locale Emulator 转区启动《{}》", game.title),
            (true, false) => format!("已通过 Locale Emulator 转区启动《{}》", game.title),
            (false, true) => format!("已提权启动《{}》", game.title),
            (false, false) => format!("已启动《{}》", game.title),
        },
        pid: if pid == 0 { None } else { Some(pid) },
        via_locale_emulator: use_le,
        tracking: true,
    })
}

/// 轮询进程是否仍存活，退出时结算时长。
///
/// 采用 `sysinfo` 按进程名匹配而非 PID：经 LE 启动时，LEProc 会再拉起真正的
/// 游戏进程，PID 与父进程并不一致；部分游戏还会自我重启，按名字匹配更稳。
///
/// 但**只按名字匹配会误判**：大量 Galgame 的可执行文件都叫 `Game.exe`，
/// 若用子串匹配，`steamgame.exe`、`mygame.exe` 之类的无关进程会让会话永不结算。
/// 因此这里要求进程名**完全相等**，并在能取到进程路径时再用「预期 exe 或同目录」二次校验；
/// 提权启动的进程往往查不到路径（`exe()` 返回 None），此时退回仅按名字判断。
fn spawn_monitor(
    app: AppHandle,
    db: Db,
    launcher: LauncherState,
    session: RunningGame,
    expected_exe: PathBuf,
    game_dir: PathBuf,
) {
    std::thread::spawn(move || {
        use sysinfo::{ProcessesToUpdate, System};

        // 提权进程查不到 exe() 时退化为纯名字匹配；能查到则要求落在预期位置
        let expected_exe_norm = normalize_path(&expected_exe);
        let game_dir_norm = normalize_path(&game_dir);

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
                // 1) 进程名必须完全相等（此前用 contains 会误匹配 steamgame.exe 等）
                if process.name().to_string_lossy().to_lowercase() != session.process_name {
                    return false;
                }
                // 2) 能拿到进程路径时再校验一次，排除「别的游戏也叫 Game.exe」
                let Some(actual) = process.exe() else {
                    return true; // 提权进程通常查不到路径，退回仅按名字判断
                };
                let actual = normalize_path(actual);
                if actual == expected_exe_norm {
                    return true;
                }
                // 兼容游戏自我重启 / 释放到子目录后再启动的情况
                actual.starts_with(game_dir_norm.as_str())
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
        // 非转区启动时 PID 就是游戏进程本身，优先按 PID 精确结束，
        // 避免 `taskkill /IM game.exe` 把同名的其它游戏一起杀掉。
        if !session.via_locale_emulator && session.pid != 0 {
            let ok = Command::new("taskkill")
                .args(["/F", "/PID", &session.pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            if ok {
                terminated = 1;
            }
        }

        // 转区启动时 PID 是 LEProc 的、提权启动时拿不到 PID，都退回按进程名结束
        if terminated == 0 {
            let output = Command::new("taskkill")
                .args(["/F", "/IM", &session.process_name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output();
            if output.map(|o| o.status.success()).unwrap_or(false) {
                terminated = 1;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- split_args ----------

    #[test]
    fn split_args_keeps_quoted_path_as_one_token() {
        assert_eq!(split_args(r#"-runas ja-JP "D:/My Games/Game.exe""#), ["-runas", "ja-JP", "D:/My Games/Game.exe"]
        );
    }

    #[test]
    fn split_args_handles_plain_and_empty_input() {
        assert_eq!(split_args("a b  c"), ["a", "b", "c"]);
        assert!(split_args("   ").is_empty());
        assert!(split_args("").is_empty());
    }

    #[test]
    fn split_args_keeps_inner_quotes_content() {
        assert_eq!(split_args(r#""a b" "c d""#), ["a b", "c d"]);
    }

    // ---------- render_template ----------

    /// 回归：路径含空格时必须仍然只产生一个参数。
    /// 旧实现先替换再 `split_whitespace()`，会把 `D:/Games/My Galgame/Game.exe`
    /// 拆成 `D:/Games/My` 和 `Galgame/Game.exe`，导致 LE 启动失败。
    #[test]
    fn render_template_default_template_keeps_spaced_exe_atomic() {
        let got = render_template(
            DEFAULT_LE_TEMPLATE,
            "D:/Games/My Galgame/Game.exe",
            None,
            "ja-JP",
            "D:/Games/My Galgame",
        );
        assert_eq!(got, ["D:/Games/My Galgame/Game.exe"]);
    }

    #[test]
    fn render_template_explicit_locale_with_spaced_exe() {
        let got = render_template(
            r#"-runas {locale} "{exe}""#,
            "D:/Games/My Galgame/Game.exe",
            None,
            "ja-JP",
            "D:/Games/My Galgame",
        );
        assert_eq!(got, ["-runas", "ja-JP", "D:/Games/My Galgame/Game.exe"]);
    }

    #[test]
    fn render_template_unquoted_placeholder_still_atomic() {
        // 即使用户没写引号，含空格的 {exe} 也不该被拆开
        let got = render_template("{exe}", "C:/Program Files/Game/game.exe", None, "", "");
        assert_eq!(got, ["C:/Program Files/Game/game.exe"]);
    }

    #[test]
    fn render_template_args_placeholder_expands_to_multiple() {
        let got = render_template("{exe} {args}", "C:/g/game.exe", Some("-w 1920 -h 1080"), "", "");
        assert_eq!(got, ["C:/g/game.exe", "-w", "1920", "-h", "1080"]);
    }

    #[test]
    fn render_template_args_empty_contributes_nothing() {
        let got = render_template("{exe} {args}", "C:/g/game.exe", None, "", "");
        assert_eq!(got, ["C:/g/game.exe"]);
    }

    #[test]
    fn render_template_dir_placeholder_with_spaces() {
        let got = render_template("{dir}", "x", None, "", "D:/Games/My Galgame");
        assert_eq!(got, ["D:/Games/My Galgame"]);
    }

    // ---------- normalize_path ----------

    #[test]
    fn normalize_path_unifies_separators_and_case() {
        assert_eq!(normalize_path(Path::new(r"D:\Games\A\Game.EXE")), r"d:\games\a\game.exe");
        assert_eq!(normalize_path(Path::new("D:/Games/A/Game.exe")), r"d:\games\a\game.exe");
    }

    #[test]
    fn normalize_path_strips_extended_prefix() {
        assert_eq!(normalize_path(Path::new(r"\\?\D:\Games\Game.exe")), r"d:\games\game.exe");
    }

    /// 回归：同目录判断必须能识别「游戏自我重启后落在子目录」的情况
    #[test]
    fn normalize_path_supports_directory_prefix_check() {
        let dir = normalize_path(Path::new(r"D:\Games\A"));
        let child = normalize_path(Path::new(r"D:\Games\A\tmp\Game.exe"));
        assert!(child.starts_with(dir.as_str()));
    }

    /// 反向：不同目录的同名进程不应被判为同一游戏
    #[test]
    fn normalize_path_rejects_other_directory() {
        let dir = normalize_path(Path::new(r"D:\Games\A"));
        let other = normalize_path(Path::new(r"D:\Games\B\Game.exe"));
        assert!(!other.starts_with(dir.as_str()));
    }
}
