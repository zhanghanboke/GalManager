//! 存档自动定时备份。
//!
//! 后台线程每分钟醒一次，到点后遍历游戏库，只备份**自上次自动备份以来内容有变化**
//! 的存档目录——通过比对「文件名 + 大小 + 修改时间」算出的指纹判断，
//! 避免反复写入完全相同的归档。
//!
//! 用独立 std 线程而非 tokio 任务：定时器频率极低（分钟级），
//! 且备份是纯阻塞 IO，放在线程里不会占用异步运行时。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;
use crate::save_backup;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 设置项：是否开启
pub const KEY_ENABLED: &str = "auto_backup_saves";
/// 设置项：间隔（分钟）
pub const KEY_INTERVAL: &str = "auto_backup_interval_minutes";
/// 设置项：上次执行时间（仅用于展示）
pub const KEY_LAST_RUN: &str = "auto_backup_last_run";

/// 默认间隔：3 小时
pub const DEFAULT_INTERVAL_MINUTES: i64 = 180;
/// 允许的最小间隔，避免用户填 1 导致频繁打包
pub const MIN_INTERVAL_MINUTES: i64 = 10;

/// 调度线程的轮询间隔
const TICK: Duration = Duration::from_secs(60);

/// 同一时刻只允许一次自动备份在跑（避免与「立即备份」重叠）
static RUNNING: AtomicBool = AtomicBool::new(false);

const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// 单个游戏的自动备份结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoBackupItem {
    pub game_id: i64,
    pub title: String,
    /// backed_up | failed
    pub status: String,
    pub message: String,
}

/// 一次自动备份的汇总
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoBackupReport {
    /// 检查过的游戏数
    pub checked: usize,
    /// 实际写入新归档的数量
    pub backed_up: usize,
    /// 存档内容没变、跳过的数量
    pub unchanged: usize,
    /// 没找到存档目录、跳过的数量
    pub skipped: usize,
    pub failed: usize,
    /// 只列出真正发生了备份或失败的游戏，避免几百条「无存档」噪声
    pub details: Vec<AutoBackupItem>,
    pub started_at: String,
    pub finished_at: String,
}

/// 自动备份状态（设置页展示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoBackupStatus {
    pub enabled: bool,
    pub interval_minutes: i64,
    pub last_run_at: Option<String>,
    /// 已记录指纹的游戏数
    pub tracked_games: i64,
    pub running: bool,
}

// ==================== 设置读取 ====================

fn read_bool(db: &Db, key: &str, fallback: bool) -> bool {
    match db.get_setting(key) {
        Ok(Some(value)) => value != "false",
        _ => fallback,
    }
}

/// 读取间隔并夹到合法范围
pub fn read_interval_minutes(db: &Db) -> i64 {
    let raw = db
        .get_setting(KEY_INTERVAL)
        .ok()
        .flatten()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_MINUTES);
    raw.max(MIN_INTERVAL_MINUTES)
}

/// 距离上次执行过了多少分钟；从未执行过返回 `None`
fn minutes_since_last_run(db: &Db) -> Option<i64> {
    let raw = db.get_setting(KEY_LAST_RUN).ok().flatten()?;
    let parsed = chrono::NaiveDateTime::parse_from_str(raw.trim(), TIME_FORMAT).ok()?;
    Some((chrono::Local::now().naive_local() - parsed).num_minutes())
}

pub fn status(db: &Db) -> AppResult<AutoBackupStatus> {
    Ok(AutoBackupStatus {
        enabled: read_bool(db, KEY_ENABLED, false),
        interval_minutes: read_interval_minutes(db),
        last_run_at: db.get_setting(KEY_LAST_RUN)?.filter(|v| !v.trim().is_empty()),
        tracked_games: db.auto_backup_tracked_count()?,
        running: RUNNING.load(Ordering::SeqCst),
    })
}

// ==================== 指纹 ====================

/// FNV-1a：不引入额外依赖，且跨编译版本结果稳定
/// （`DefaultHasher` 不保证跨版本一致，会导致升级后全量重备一次）。
fn stable_hash(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 计算存档目录的内容指纹：相对路径 + 大小 + 修改时间，排序后哈希。
///
/// 目录不存在或没有文件时返回 `None`（调用方据此跳过）。
pub fn fingerprint(dir: &std::path::Path) -> Option<String> {
    if !dir.is_dir() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .max_depth(16)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let meta = entry.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let relative = entry.path().strip_prefix(dir).unwrap_or(entry.path());
        parts.push(format!(
            "{}|{}|{}",
            relative.to_string_lossy().replace('\\', "/"),
            size,
            mtime
        ));
    }
    if parts.is_empty() {
        return None;
    }
    parts.sort();
    let joined = parts.join("\n");
    Some(format!("{:016x}:{}", stable_hash(&joined), parts.len()))
}

// ==================== 执行 ====================

/// 遍历游戏库执行一次自动备份。
///
/// 只备份指纹发生变化的存档目录；备份成功后记录新指纹，
/// 失败则不记录，下次会重试。
pub fn run_once(db: &Db, paths: &AppPaths) -> AppResult<AutoBackupReport> {
    // 与手动「立即备份」互斥，避免同一时间写两份归档
    if RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::msg("自动备份正在进行中，请稍后再试"));
    }
    let result = run_once_inner(db, paths);
    RUNNING.store(false, Ordering::SeqCst);

    if let Ok(report) = &result {
        let _ = db.set_setting(
            KEY_LAST_RUN,
            &chrono::Local::now().format(TIME_FORMAT).to_string(),
        );
        log::info!(
            "自动备份完成：检查 {} 个，新增备份 {} 个，内容未变 {} 个，无存档 {} 个，失败 {} 个",
            report.checked,
            report.backed_up,
            report.unchanged,
            report.skipped,
            report.failed
        );
    }
    result
}

fn run_once_inner(db: &Db, paths: &AppPaths) -> AppResult<AutoBackupReport> {
    let games = db.list_games(&crate::models::GameFilter::default())?;
    let mut report = AutoBackupReport {
        started_at: chrono::Local::now().format(TIME_FORMAT).to_string(),
        ..Default::default()
    };

    for game in games {
        report.checked += 1;

        let probe = match save_backup::probe_game(db, game.id) {
            Ok(probe) => probe,
            Err(error) => {
                report.failed += 1;
                report.details.push(AutoBackupItem {
                    game_id: game.id,
                    title: game.title.clone(),
                    status: "failed".to_string(),
                    message: format!("探测存档目录失败: {error}"),
                });
                continue;
            }
        };

        let Some(candidate) = save_backup::best_candidate(&probe) else {
            // 多数游戏本来就没有可识别的存档目录，属于正常情况，不记入 details
            report.skipped += 1;
            continue;
        };

        let dir = std::path::PathBuf::from(&candidate.path);
        let Some(current) = fingerprint(&dir) else {
            report.skipped += 1;
            continue;
        };

        // 内容没变就跳过，避免反复写入同样的归档
        if db.auto_backup_fingerprint(game.id)?.as_deref() == Some(current.as_str()) {
            report.unchanged += 1;
            continue;
        }

        match save_backup::backup_one(
            db,
            paths,
            game.id,
            &candidate.path,
            "自动备份",
            Some("定时自动备份"),
        ) {
            Ok(slot) => {
                db.set_auto_backup_fingerprint(game.id, &current)?;
                report.backed_up += 1;
                report.details.push(AutoBackupItem {
                    game_id: game.id,
                    title: game.title.clone(),
                    status: "backed_up".to_string(),
                    message: format!("已备份 {} 个文件", slot.file_count),
                });
            }
            Err(error) => {
                report.failed += 1;
                report.details.push(AutoBackupItem {
                    game_id: game.id,
                    title: game.title.clone(),
                    status: "failed".to_string(),
                    message: format!("备份失败: {error}"),
                });
            }
        }
    }

    report.finished_at = chrono::Local::now().format(TIME_FORMAT).to_string();
    Ok(report)
}

// ==================== 调度 ====================

/// 启动后台调度线程。应用启动时调用一次。
pub fn spawn(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("galmanager-autobackup".to_string())
        .spawn(move || {
            loop {
                std::thread::sleep(TICK);

                let (enabled, db, paths) = {
                    let db = app.state::<Db>();
                    let paths = app.state::<AppPaths>();
                    (
                        read_bool(&db, KEY_ENABLED, false),
                        db.inner().clone(),
                        paths.inner().clone(),
                    )
                };

                if !enabled {
                    continue;
                }

                // 从未执行过时视为已到期：用户刚打开开关就尽快跑一次
                let due = match minutes_since_last_run(&db) {
                    Some(elapsed) => elapsed >= read_interval_minutes(&db),
                    None => true,
                };
                if !due {
                    continue;
                }

                match run_once(&db, &paths) {
                    Ok(report) => {
                        let _ = app.emit("auto-backup-finished", &report);
                    }
                    // 上一次还没跑完属于正常情况，不刷错误日志
                    Err(error) => log::debug!("本次自动备份跳过: {error}"),
                }
            }
        });

    if let Err(error) = spawned {
        log::warn!("自动备份线程启动失败（不影响其他功能）: {error}");
    } else {
        log::info!("存档自动备份调度已启动");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "galmanager_autobak_{tag}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn fingerprint_is_none_for_missing_or_empty_dir() {
        assert!(fingerprint(std::path::Path::new("Z:/definitely/not/here")).is_none());
        let dir = temp_dir("empty");
        assert!(fingerprint(&dir).is_none(), "空目录不该产生指纹");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 指纹必须能反映内容变化，否则自动备份会漏掉新存档
    #[test]
    fn fingerprint_changes_when_file_content_changes() {
        let dir = temp_dir("change");
        std::fs::write(dir.join("save001.dat"), b"first").unwrap();
        let before = fingerprint(&dir).unwrap();

        // 同样文件名、不同大小 → 指纹必须变
        std::fs::write(dir.join("save001.dat"), b"second-and-longer").unwrap();
        let after = fingerprint(&dir).unwrap();
        assert_ne!(before, after, "文件内容变化后指纹应改变");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fingerprint_changes_when_file_added() {
        let dir = temp_dir("add");
        std::fs::write(dir.join("a.dat"), b"1").unwrap();
        let before = fingerprint(&dir).unwrap();

        std::fs::write(dir.join("b.dat"), b"2").unwrap();
        let after = fingerprint(&dir).unwrap();
        assert_ne!(before, after, "新增存档文件后指纹应改变");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 同一份内容重复计算结果必须一致，否则每次都会重复备份
    #[test]
    fn fingerprint_is_stable_for_unchanged_dir() {
        let dir = temp_dir("stable");
        std::fs::write(dir.join("a.dat"), b"abc").unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/b.dat"), b"def").unwrap();

        assert_eq!(fingerprint(&dir), fingerprint(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn interval_is_clamped_to_minimum() {
        let path = std::env::temp_dir().join(format!(
            "galmanager_autobak_cfg_{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).unwrap();

        // 未设置时用默认值
        assert_eq!(read_interval_minutes(&db), DEFAULT_INTERVAL_MINUTES);

        // 填了过小的值要被抬到下限，避免频繁打包
        db.set_setting(KEY_INTERVAL, "1").unwrap();
        assert_eq!(read_interval_minutes(&db), MIN_INTERVAL_MINUTES);

        // 合法值原样生效
        db.set_setting(KEY_INTERVAL, "240").unwrap();
        assert_eq!(read_interval_minutes(&db), 240);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn status_reflects_settings() {
        let path = std::env::temp_dir().join(format!(
            "galmanager_autobak_status_{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).unwrap();

        let initial = status(&db).unwrap();
        assert!(!initial.enabled, "默认应关闭");
        assert_eq!(initial.tracked_games, 0);

        db.set_setting(KEY_ENABLED, "true").unwrap();
        db.set_setting(KEY_INTERVAL, "60").unwrap();
        let updated = status(&db).unwrap();
        assert!(updated.enabled);
        assert_eq!(updated.interval_minutes, 60);

        let _ = std::fs::remove_file(&path);
    }

    /// 端到端：首次备份 → 内容未变跳过 → 内容变化后重新备份；
    /// 同时验证没有可识别存档目录的游戏被安静跳过。
    ///
    /// 注意：`run_once` 用全局互斥量防重入（生产环境里确实只允许一次自动备份在跑），
    /// 所以整个测试集里只能有这一个用例调用它，否则会互相打架。
    #[test]
    fn run_once_backs_up_only_when_saves_change() {
        let root = temp_dir("run_root");
        let paths = AppPaths::new(root.clone()).expect("创建 AppPaths 失败");
        let db = Db::open(&root.join("galmanager.db")).expect("打开数据库失败");

        // 单独造一个存档目录，并把它手动指定给游戏，避免依赖引擎探测规则
        let save_dir = temp_dir("run_saves");
        std::fs::write(save_dir.join("save001.dat"), b"first").unwrap();
        let save_path = save_dir.to_string_lossy().to_string();

        let game_id = db
            .create_game(&crate::models::GameInput {
                title: "测试游戏".to_string(),
                path: Some(save_path.clone()),
                save_path: Some(save_path.clone()),
                engine: Some("kirikiri".to_string()),
                ..Default::default()
            })
            .expect("创建游戏失败");

        // 再放一个探不到存档目录的游戏，它应该被安静跳过而不是算作失败
        db.create_game(&crate::models::GameInput {
            title: "没有存档的游戏".to_string(),
            path: Some("Z:/definitely/not/here".to_string()),
            engine: Some("other".to_string()),
            ..Default::default()
        })
        .expect("创建游戏失败");

        // ---- 第一次：只有有存档的那个游戏被备份 ----
        let first = run_once(&db, &paths).expect("首次自动备份失败");
        assert_eq!(first.checked, 2);
        assert_eq!(first.backed_up, 1, "首次应写入备份");
        assert_eq!(first.unchanged, 0);
        assert_eq!(first.skipped, 1, "无存档的游戏应被跳过");
        assert_eq!(first.failed, 0, "无存档不算失败");
        assert!(
            first.details.iter().all(|d| d.game_id == game_id),
            "明细里不应出现无存档游戏的噪声"
        );
        assert_eq!(db.list_save_slots(game_id).unwrap().len(), 1);

        // ---- 第二次：存档没动 → 必须跳过，不能重复写归档 ----
        let second = run_once(&db, &paths).expect("第二次自动备份失败");
        assert_eq!(second.backed_up, 0);
        assert_eq!(second.unchanged, 1, "内容未变应计入 unchanged");
        assert_eq!(
            db.list_save_slots(game_id).unwrap().len(),
            1,
            "内容未变时不应新增槽位"
        );

        // ---- 第三次：存档有变化 → 重新备份 ----
        std::fs::write(save_dir.join("save002.dat"), b"second").unwrap();
        let third = run_once(&db, &paths).expect("第三次自动备份失败");
        assert_eq!(third.backed_up, 1, "内容变化后应重新备份");
        let slots = db.list_save_slots(game_id).unwrap();
        assert_eq!(slots.len(), 2);
        // 两次备份必须落在不同的归档文件上，否则第二次会覆盖第一次
        assert_ne!(
            slots[0].backup_path, slots[1].backup_path,
            "两次备份不应指向同一个归档文件"
        );
        for slot in &slots {
            assert!(
                std::path::Path::new(&slot.backup_path).is_file(),
                "归档文件应真实存在: {}",
                slot.backup_path
            );
        }

        // ---- 记录指纹后再跑一次，仍应跳过 ----
        let fourth = run_once(&db, &paths).expect("第四次自动备份失败");
        assert_eq!(fourth.backed_up, 0);
        assert_eq!(fourth.unchanged, 1);
        assert_eq!(db.auto_backup_tracked_count().unwrap(), 1);

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&save_dir);
    }
}
