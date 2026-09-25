//! 存档管理：路径识别、备份、还原、槽位信息。
//!
//! 备份格式为 zip 归档，内部结构：
//! ```text
//! manifest.json          # 备份元信息（游戏、引擎、来源路径、时间）
//! payload/...            # 原始存档文件树
//! ```
//! 这样即使脱离本应用，用户也能用任意解压工具取回存档。

use crate::engine;
use crate::error::{AppError, AppResult};
use crate::models::{SavePathCandidate, SavePathProbe};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// 备份归档内的清单文件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub game_id: i64,
    pub game_title: String,
    pub engine: Option<String>,
    pub source_path: String,
    pub created_at: String,
    pub file_count: i64,
    pub size_bytes: i64,
    pub remark: Option<String>,
    pub app_version: String,
}

const MANIFEST_NAME: &str = "manifest.json";
const PAYLOAD_DIR: &str = "payload";

/// 展开 Windows 环境变量占位符，如 `%APPDATA%`。
pub fn expand_env(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let bytes: Vec<char> = raw.chars().collect();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == '%' {
            if let Some(end) = bytes[index + 1..].iter().position(|&c| c == '%') {
                let key: String = bytes[index + 1..index + 1 + end].iter().collect();
                if let Ok(value) = std::env::var(&key) {
                    result.push_str(&value);
                    index += end + 2;
                    continue;
                }
            }
        }
        result.push(bytes[index]);
        index += 1;
    }
    result
}

fn appdata() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(PathBuf::from)
}

fn localappdata() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA").ok().map(PathBuf::from)
}

fn userprofile() -> Option<PathBuf> {
    std::env::var("USERPROFILE").ok().map(PathBuf::from)
}

/// 统计目录内文件数与总字节数（带深度上限，避免扫描失控）
pub fn measure_dir(dir: &Path) -> (i64, i64) {
    if !dir.is_dir() {
        return (0, 0);
    }
    let mut count = 0i64;
    let mut size = 0i64;
    for entry in walkdir::WalkDir::new(dir)
        .max_depth(8)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            count += 1;
            size += entry.metadata().map(|m| m.len() as i64).unwrap_or(0);
        }
    }
    (count, size)
}

fn candidate(path: PathBuf, source: &str) -> SavePathCandidate {
    let exists = path.is_dir();
    let (file_count, size_bytes) = if exists { measure_dir(&path) } else { (0, 0) };
    SavePathCandidate {
        path: path.to_string_lossy().to_string(),
        exists,
        file_count,
        size_bytes,
        source: source.to_string(),
    }
}

/// 从 `game/options.rpy` 中解析 `config.save_directory`
fn renpy_save_directory(game_path: &Path) -> Option<String> {
    let options = game_path.join("game").join("options.rpy");
    let content = std::fs::read_to_string(&options).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("define config.save_directory") {
            continue;
        }
        let after_eq = trimmed.split('=').nth(1)?.trim();
        let value = after_eq.trim_matches(|c| c == '"' || c == '\'' || c == ' ' || c == '\t');
        let cleaned = value
            .replace("u\"", "")
            .replace("u'", "")
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string();
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }
    None
}

/// 从 Unity 的 `app.info` 中读取「公司名 / 产品名」。
/// 该文件是两行纯文本，可直接按行解析。
fn unity_app_info(game_path: &Path) -> Option<(String, String)> {
    let entries = std::fs::read_dir(game_path).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.to_lowercase().ends_with("_data") {
            continue;
        }
        let info = entry.path().join("app.info");
        let content = std::fs::read_to_string(&info).ok()?;
        let mut lines = content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty());
        let company = lines.next()?;
        let product = lines.next().unwrap_or_else(|| company.clone());
        return Some((company, product));
    }
    None
}

/// 探测某个游戏的存档路径。
///
/// 返回按引擎特性生成的候选列表，**不会**修改任何文件。
pub fn probe(
    game_path: Option<&str>,
    engine_id: Option<&str>,
    title: &str,
    executable: Option<&str>,
) -> AppResult<SavePathProbe> {
    let engine_key = engine_id.unwrap_or("other");
    // 路径可能带 `%APPDATA%` 这类占位符，先展开再判断
    let game_dir = game_path
        .map(|p| PathBuf::from(expand_env(p)))
        .filter(|p| p.is_dir());
    let exe_stem = executable
        .map(|e| {
            Path::new(e)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| e.to_string())
        })
        .unwrap_or_else(|| title.to_string());

    let mut paths: Vec<SavePathCandidate> = Vec::new();
    let mut confidence = 0i64;

    // ---- 引擎专属规则 ----
    match engine_key {
        "kirikiri" | "siglus" | "bgi" | "artemis" | "majiro" | "systemnnn" | "catystem" => {
            if let Some(dir) = &game_dir {
                paths.push(candidate(dir.join("savedata"), "游戏目录/savedata"));
                paths.push(candidate(dir.join("save"), "游戏目录/save"));
                paths.push(candidate(dir.join("savedata").join(&exe_stem), "游戏目录/savedata/<exe>"));
            }
            if let Some(base) = appdata() {
                paths.push(candidate(base.join(&exe_stem), "%APPDATA%/<exe>"));
                paths.push(candidate(base.join(title), "%APPDATA%/<游戏名>"));
            }
            confidence = 60;
        }
        "renpy" => {
            if let Some(dir) = &game_dir {
                // 优先使用 options.rpy 里声明的目录（Ren'Py 会把它拼到 %APPDATA%/RenPy 下）
                if let Some(declared) = renpy_save_directory(dir) {
                    let declared_clean = declared.trim_start_matches('~').trim_matches('/');
                    if let Some(base) = appdata() {
                        paths.push(candidate(
                            base.join("RenPy").join(declared_clean),
                            "options.rpy 声明的 save_directory",
                        ));
                    }
                    confidence = 85;
                }
                paths.push(candidate(dir.join("game").join("saves"), "游戏目录/game/saves"));
                if let Some(base) = appdata() {
                    paths.push(candidate(
                        base.join("RenPy").join(title),
                        "%APPDATA%/RenPy/<游戏名>",
                    ));
                }
            }
            confidence = confidence.max(55);
        }
        "unity" => {
            if let Some(dir) = &game_dir {
                if let Some((company, product)) = unity_app_info(dir) {
                    if let Some(base) = userprofile() {
                        paths.push(candidate(
                            base.join("AppData")
                                .join("LocalLow")
                                .join(&company)
                                .join(&product),
                            "app.info 解析出的 LocalLow 路径",
                        ));
                    }
                    confidence = 85;
                }
                paths.push(candidate(dir.join("savedata"), "游戏目录/savedata"));
                paths.push(candidate(dir.join("save"), "游戏目录/save"));
            }
            if let Some(base) = userprofile() {
                let local_low = base.join("AppData").join("LocalLow");
                paths.push(candidate(local_low.join(title), "LocalLow/<游戏名>"));
                if let Ok(entries) = std::fs::read_dir(&local_low) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.to_lowercase().contains(&title.to_lowercase())
                            && !title.trim().is_empty()
                        {
                            paths.push(candidate(entry.path(), "LocalLow 模糊匹配"));
                        }
                    }
                }
            }
            confidence = confidence.max(50);
        }
        "rpgmaker" => {
            if let Some(dir) = &game_dir {
                paths.push(candidate(dir.clone(), "游戏目录（Save*.rvdata 就地保存）"));
                paths.push(candidate(dir.join("Save"), "游戏目录/Save"));
            }
            confidence = 70;
        }
        "rpgmaker_mv" => {
            if let Some(dir) = &game_dir {
                paths.push(candidate(dir.join("www").join("save"), "游戏目录/www/save"));
                paths.push(candidate(dir.join("save"), "游戏目录/save"));
            }
            if let Some(base) = localappdata() {
                paths.push(candidate(
                    base.join(title).join("User Data").join("Default"),
                    "%LOCALAPPDATA%/<游戏名>/User Data/Default",
                ));
                paths.push(candidate(base.join(&exe_stem), "%LOCALAPPDATA%/<exe>"));
            }
            confidence = 65;
        }
        "nscripter" | "reallive" => {
            if let Some(dir) = &game_dir {
                paths.push(candidate(dir.clone(), "游戏目录（就地保存）"));
                paths.push(candidate(dir.join("save"), "游戏目录/save"));
            }
            confidence = 60;
        }
        "wolfrpg" => {
            if let Some(dir) = &game_dir {
                paths.push(candidate(
                    dir.join("Data").join("SaveData"),
                    "游戏目录/Data/SaveData",
                ));
                paths.push(candidate(dir.join("SaveData"), "游戏目录/SaveData"));
            }
            if let Some(base) = appdata() {
                paths.push(candidate(base.join(title), "%APPDATA%/<游戏名>"));
            }
            confidence = 70;
        }
        "unreal" => {
            if let Some(base) = localappdata() {
                paths.push(candidate(
                    base.join(title).join("Saved").join("SaveGames"),
                    "%LOCALAPPDATA%/<游戏名>/Saved/SaveGames",
                ));
            }
            confidence = 55;
        }
        _ => {
            confidence = 20;
        }
    }

    // ---- 通用兜底：所有引擎都检查常见位置 ----
    if let Some(dir) = &game_dir {
        paths.push(candidate(dir.join("save"), "通用：游戏目录/save"));
        paths.push(candidate(dir.join("saves"), "通用：游戏目录/saves"));
        paths.push(candidate(dir.join("savedata"), "通用：游戏目录/savedata"));
    }
    if let Some(base) = appdata() {
        paths.push(candidate(base.join(title), "通用：%APPDATA%/<游戏名>"));
    }
    if let Some(base) = localappdata() {
        paths.push(candidate(base.join(title), "通用：%LOCALAPPDATA%/<游戏名>"));
    }
    if let Some(base) = userprofile() {
        paths.push(candidate(
            base.join("Documents").join(title),
            "通用：文档/<游戏名>",
        ));
    }

    // 去重（同一路径只保留来源描述最靠前的一条），并优先展示「存在且非空」的目录
    let mut seen = std::collections::HashSet::new();
    let mut deduped: Vec<SavePathCandidate> = Vec::new();
    for item in paths {
        let key = item.path.to_lowercase();
        if seen.insert(key) {
            deduped.push(item);
        }
    }
    deduped.sort_by(|a, b| {
        let rank = |c: &SavePathCandidate| -> i32 {
            if c.exists && c.file_count > 0 {
                0
            } else if c.exists {
                1
            } else {
                2
            }
        };
        rank(a).cmp(&rank(b)).then(b.file_count.cmp(&a.file_count))
    });
    deduped.truncate(12);

    let hit_count = deduped
        .iter()
        .filter(|c| c.exists && c.file_count > 0)
        .count();
    let final_confidence = if hit_count > 0 {
        confidence.max(70)
    } else {
        confidence
    };

    Ok(SavePathProbe {
        engine: engine_key.to_string(),
        engine_label: engine::label_of(engine_key),
        confidence: final_confidence,
        paths: deduped,
    })
}

/// 备份结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOutcome {
    pub backup_path: String,
    pub size_bytes: i64,
    pub file_count: i64,
}

/// 把 `source` 目录打包成 zip 归档。
pub fn create_backup(
    game_id: i64,
    game_title: &str,
    engine_id: Option<&str>,
    source: &Path,
    dest_archive: &Path,
    remark: Option<&str>,
) -> AppResult<BackupOutcome> {
    if !source.is_dir() {
        return Err(AppError::msg(format!(
            "存档目录不存在: {}",
            source.display()
        )));
    }
    if let Some(parent) = dest_archive.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let (file_count, size_bytes) = measure_dir(source);
    if file_count == 0 {
        return Err(AppError::msg(format!(
            "存档目录为空，未创建备份: {}",
            source.display()
        )));
    }

    let manifest = BackupManifest {
        game_id,
        game_title: game_title.to_string(),
        engine: engine_id.map(|s| s.to_string()),
        source_path: source.to_string_lossy().to_string(),
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        file_count,
        size_bytes,
        remark: remark.map(|s| s.to_string()),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let file = File::create(dest_archive)?;
    let mut writer = zip::ZipWriter::new(file);
    let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    // 写入清单
    writer.start_file(MANIFEST_NAME, options)?;
    writer.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    // 写入文件树
    let source_abs = source.to_path_buf();
    for entry in walkdir::WalkDir::new(&source_abs)
        .max_depth(16)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        let path = entry.path();
        let relative = match path.strip_prefix(&source_abs) {
            Ok(rel) => rel,
            Err(_) => continue,
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let archive_name = format!(
            "{}/{}",
            PAYLOAD_DIR,
            relative.to_string_lossy().replace('\\', "/")
        );

        if entry.file_type().is_dir() {
            writer.add_directory(archive_name, options)?;
        } else if entry.file_type().is_file() {
            writer.start_file(archive_name, options)?;
            let mut input = File::open(path)?;
            let mut buffer = Vec::with_capacity(64 * 1024);
            input.read_to_end(&mut buffer)?;
            writer.write_all(&buffer)?;
        }
    }

    writer.finish()?;

    let archived_size = std::fs::metadata(dest_archive).map(|m| m.len() as i64).unwrap_or(0);
    log::info!(
        "存档备份完成 game_id={} 来源={} 归档={} 文件数={} 原始={}B 压缩后={}B",
        game_id,
        source.display(),
        dest_archive.display(),
        file_count,
        size_bytes,
        archived_size
    );

    Ok(BackupOutcome {
        backup_path: dest_archive.to_string_lossy().to_string(),
        size_bytes: archived_size,
        file_count,
    })
}

/// 读取归档内的清单信息（不落地任何文件）
pub fn read_manifest(archive_path: &Path) -> AppResult<BackupManifest> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut entry = archive
        .by_name(MANIFEST_NAME)
        .map_err(|_| AppError::msg("备份文件缺少 manifest.json，可能不是本应用生成的归档"))?;
    let mut content = String::new();
    entry.read_to_string(&mut content)?;
    Ok(serde_json::from_str(&content)?)
}

/// 还原选项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOptions {
    pub archive_path: String,
    /// 目标目录（通常为探测到的存档目录）
    pub target_path: String,
    /// 还原前是否把现有存档另存为 `.before_restore_<时间戳>` 备份。
    ///
    /// 还原是**完全还原**：会先清空目标目录再解压归档，以保证结果精确等于备份时刻的状态。
    /// 因此当目标目录非空时，本项必须为 `true`，否则直接报错拒绝执行，避免误删。
    #[serde(default = "default_true")]
    pub backup_existing: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    pub restored_files: i64,
    pub safety_backup: Option<String>,
    pub target_path: String,
}

/// 从归档**完全还原**存档到目标目录。
///
/// 语义是「让目标目录精确回到备份时刻」：先按需留安全备份，再清空目标目录，
/// 最后解压归档。若目标目录非空且未开启安全备份，则拒绝执行。
pub fn restore(options: &RestoreOptions) -> AppResult<RestoreOutcome> {
    let archive_path = PathBuf::from(&options.archive_path);
    if !archive_path.is_file() {
        return Err(AppError::msg(format!(
            "备份文件不存在: {}",
            archive_path.display()
        )));
    }
    let target = PathBuf::from(&options.target_path);
    if target.as_os_str().is_empty() {
        return Err(AppError::msg("未指定还原目标目录"));
    }

    // 安全兜底：还原前把目标目录整体另存一份
    let mut safety_backup = None;
    let mut target_had_content = false;
    if target.is_dir() {
        let (count, _) = measure_dir(&target);
        target_had_content = count > 0;

        if options.backup_existing && count > 0 {
            let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
            let sibling = target.with_file_name(format!(
                "{}.before_restore_{}",
                target
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "savedata".into()),
                stamp
            ));
            copy_dir_recursive(&target, &sibling)?;
            log::warn!(
                "还原前已将现有存档另存: {} -> {}",
                target.display(),
                sibling.display()
            );
            safety_backup = Some(sibling.to_string_lossy().to_string());
        }
    }

    // 「完全还原」：先清空目标目录再解压。
    // 若只做覆盖式写入，归档里没有、而目标里仍存在的文件会残留
    // （例如备份之后新增的存档槽位），用户就无法真正回退到备份时刻的状态。
    if target_had_content {
        if safety_backup.is_none() {
            // 没有安全备份却要清空一个非空目录，风险太高，直接拒绝
            return Err(AppError::msg(format!(
                "目标目录非空（{}），完全还原会先清空该目录，因此必须先做安全备份。\
                 请开启「还原前自动备份」后重试。",
                target.display()
            )));
        }
        clear_dir_contents(&target)?;
        log::info!("已清空目标目录以执行完全还原: {}", target.display());
    }

    std::fs::create_dir_all(&target)?;

    let file = File::open(&archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let prefix = format!("{}/", PAYLOAD_DIR);
    let mut restored = 0i64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let raw_name = entry.name().to_string();
        let relative = match raw_name.strip_prefix(&prefix) {
            Some(rel) if !rel.is_empty() => rel.to_string(),
            _ => continue, // 跳过 manifest.json 等元数据
        };

        // 防目录穿越：拒绝 .. 与绝对路径
        let relative_path = Path::new(&relative);
        if relative_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir))
        {
            log::warn!("跳过可疑归档条目: {}", raw_name);
            continue;
        }

        let out_path = target.join(relative_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&out_path)?;
        std::io::copy(&mut entry, &mut output)?;
        restored += 1;
    }

    log::info!(
        "存档还原完成 归档={} 目标={} 还原文件数={}",
        archive_path.display(),
        target.display(),
        restored
    );

    Ok(RestoreOutcome {
        restored_files: restored,
        safety_backup,
        target_path: target.to_string_lossy().to_string(),
    })
}

/// 清空目录内的所有条目，但保留目录本身。
///
/// 用于「完全还原」前抹掉旧存档。调用方必须已确保做过安全备份。
fn clear_dir_contents(dir: &Path) -> AppResult<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> AppResult<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&source, &dest)?;
        } else {
            std::fs::copy(&source, &dest)?;
        }
    }
    Ok(())
}

/// 归档内容预览（还原前让用户确认）
pub fn inspect_archive(archive_path: &Path) -> AppResult<Vec<(String, i64)>> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let prefix = format!("{}/", PAYLOAD_DIR);
    let mut items = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if let Some(rel) = name.strip_prefix(&prefix) {
            if !rel.is_empty() {
                items.push((rel.to_string(), entry.size() as i64));
            }
        }
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "galmanager_test_{tag}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn expand_env_replaces_known_variable() {
        std::env::set_var("GALMANAGER_TEST_VAR", "hello");
        assert_eq!(expand_env("%GALMANAGER_TEST_VAR%/x"), "hello/x");
    }

    #[test]
    fn expand_env_keeps_unknown_placeholder() {
        assert_eq!(expand_env("%GALMANAGER_NOPE%/x"), "%GALMANAGER_NOPE%/x");
        assert_eq!(expand_env("no placeholder"), "no placeholder");
    }

    /// 完全还原依赖这个函数清空旧存档；必须递归删子目录，且不能删掉目录本身。
    #[test]
    fn clear_dir_contents_removes_entries_but_keeps_dir() {
        let dir = temp_dir("clear");
        std::fs::write(dir.join("save001.dat"), b"a").unwrap();
        std::fs::create_dir_all(dir.join("sub/deep")).unwrap();
        std::fs::write(dir.join("sub/deep/save002.dat"), b"b").unwrap();

        clear_dir_contents(&dir).unwrap();

        assert!(dir.is_dir(), "目录本身必须保留");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_dir_contents_on_empty_dir_is_ok() {
        let dir = temp_dir("clear_empty");
        clear_dir_contents(&dir).unwrap();
        assert!(dir.is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn measure_dir_counts_files_and_bytes() {
        let dir = temp_dir("measure");
        std::fs::write(dir.join("a.dat"), b"12345").unwrap();
        std::fs::create_dir_all(dir.join("s")).unwrap();
        std::fs::write(dir.join("s/b.dat"), b"67").unwrap();

        let (count, size) = measure_dir(&dir);
        assert_eq!(count, 2);
        assert_eq!(size, 7);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn measure_dir_on_missing_path_is_zero() {
        assert_eq!(measure_dir(Path::new("Z:/definitely/not/here")), (0, 0));
    }
}
