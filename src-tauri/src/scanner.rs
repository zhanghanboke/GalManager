//! 本地文件夹递归扫描与可执行文件识别。
//!
//! 核心难点在于「如何把一堆杂乱的目录识别成游戏」：
//! 1. 过滤掉系统目录、运行时目录、补丁目录；
//! 2. 过滤掉卸载程序、DirectX 安装器等非启动项；
//! 3. 对目录名做清洗（去掉 `[社团]`、`（初回版）` 这类噪声）；
//! 4. 对可执行文件排序，把最可能的启动项排在最前。

use crate::engine;
use crate::error::{AppError, AppResult};
use crate::models::{ScanCandidate, ScanOptions};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

/// 视为可执行启动项的后缀
pub const EXE_EXTENSIONS: &[&str] = &["exe", "bat", "cmd"];

/// 扫描时直接剪枝的目录名（不区分大小写）
const EXCLUDED_DIRS: &[&str] = &[
    // 系统目录
    "$recycle.bin",
    "system volume information",
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "appdata",
    "recovery",
    "perflogs",
    // 开发 / 版本控制
    ".git",
    ".svn",
    ".vs",
    ".idea",
    "node_modules",
    "__pycache__",
    "target",
    "dist",
    // 常见运行时 / 前置组件目录（不含游戏主程序）
    "redist",
    "redistributables",
    "_commonredist",
    "commonredist",
    "directx",
    "vcredist",
    "dotnet",
    "installer",
    "installers",
];

/// exe 文件名命中以下关键字则判定为非游戏启动项
const EXCLUDED_EXE_PATTERNS: &[&str] = &[
    "unins",        // 卸载程序
    "uninst",
    "uninstall",
    "setup",        // 安装向导
    "install",
    "dxsetup",      // DirectX
    "dxwebsetup",
    "vcredist",     // VC++ 运行时
    "vc_redist",
    "dotnetfx",     // .NET
    "oalinst",      // OpenAL
    "crashhandler", // 崩溃处理器
    "crash_handler",
    "crashreport",
    "crash_report",
    "bugreport",
    "bug_report",
    "unitycrashandler",
    "update",
    "updater",
    "config",
    "settings",
    "readme",
    "注册表",
    "注冊表",
    "存档目录",
    "存檔目錄",
    "解锁程序",
    "解鎖程序",
    "汉化补丁",
    "漢化補丁",
];

/// 目录名清洗时忽略的括号类型
fn bracket_delta(ch: char) -> Option<i32> {
    match ch {
        '[' | '(' | '【' | '（' | '〔' | '『' => Some(1),
        ']' | ')' | '】' | '）' | '〕' | '』' => Some(-1),
        _ => None,
    }
}

/// 去掉目录名中的方括号 / 圆括号标注，得到更适合搜索的游戏名。
///
/// 例：`[1103] 千恋万花 (完全版)` → `千恋万花`
pub fn trim_dirname_to_search_name(dir_name: &str) -> String {
    let mut result = String::with_capacity(dir_name.len());
    let mut depth = 0_i32;

    for ch in dir_name.chars() {
        if let Some(delta) = bracket_delta(ch) {
            depth = (depth + delta).max(0);
            continue;
        }
        if depth == 0 {
            result.push(ch);
        }
    }

    let trimmed = result.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        dir_name.trim().to_string()
    } else {
        trimmed
    }
}

fn is_excluded_dir(name: &str) -> bool {
    let lower = name.to_lowercase();
    EXCLUDED_DIRS.iter().any(|&d| lower == d)
}

pub fn is_excluded_exe(path: &Path) -> bool {
    match path.file_stem() {
        Some(stem) => {
            let lower = stem.to_string_lossy().to_lowercase();
            EXCLUDED_EXE_PATTERNS.iter().any(|&p| lower.contains(p))
        }
        None => false,
    }
}

fn has_valid_extension(path: &Path) -> bool {
    match path.extension() {
        Some(ext) => EXE_EXTENSIONS
            .iter()
            .any(|expected| ext.eq_ignore_ascii_case(expected)),
        None => false,
    }
}

fn is_han(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2EBEF}'
    )
}

fn is_kana(ch: char) -> bool {
    matches!(ch, '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF65}'..='\u{FF9F}')
}

/// 判断文件名是否为「中文化启动项」（含汉字且不含假名）。
/// 汉化版的 exe 往往才是玩家真正要启动的那个。
fn is_probably_chinese_exe(value: &str) -> bool {
    let file_name = Path::new(value)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| value.to_string());
    let mut contains_han = false;
    for ch in file_name.chars() {
        if is_kana(ch) {
            return false;
        }
        contains_han |= is_han(ch);
    }
    contains_han
}

/// 可执行文件排序：越可能是「真正的启动项」排越前。
///
/// 优先级：含 chs/cn 关键字 > 中文名 > 与游戏同名 > 路径短 > 字母序
pub fn sort_executables(executables: &mut [String], game_name: &str) {
    let lower_name = game_name.to_lowercase();
    executables.sort_by_cached_key(|executable| {
        let lower = executable.to_lowercase();
        let depth = executable.matches(['\\', '/']).count();
        (
            lower.contains("startup"),
            !(lower.contains("chs") || lower.contains("cn") || lower.contains("zh")),
            !is_probably_chinese_exe(executable),
            !lower_name.is_empty() && !lower.contains(&lower_name),
            depth,
            executable.len(),
            lower,
        )
    });
}

/// 列出某个目录直属的可执行启动项（已过滤 + 排序）。
pub fn list_executables(root: &Path) -> AppResult<Vec<String>> {
    if !root.is_dir() {
        return Err(AppError::msg(format!(
            "游戏目录不存在: {}",
            root.display()
        )));
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if !file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        if !has_valid_extension(&path) || is_excluded_exe(&path) {
            continue;
        }
        if let Some(name) = path.file_name() {
            candidates.push(name.to_string_lossy().to_string());
        }
    }

    let game_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    sort_executables(&mut candidates, game_name);
    Ok(candidates)
}

/// 归一化路径用于去重比较（Windows 下大小写不敏感）。
fn normalize_path(path: &Path) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(p) => parts.push(normalize_component(p.as_os_str().to_string_lossy().as_ref())),
            Component::RootDir => parts.push("/".to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            Component::Normal(v) => parts.push(normalize_component(v.to_string_lossy().as_ref())),
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

#[cfg(windows)]
fn normalize_component(value: &str) -> String {
    value.to_lowercase()
}

#[cfg(not(windows))]
fn normalize_component(value: &str) -> String {
    value.to_string()
}

/// 已导入路径索引：命中目录本身或其后代即视为重复。
pub struct ImportedPathIndex {
    paths: HashSet<Vec<String>>,
}

impl ImportedPathIndex {
    pub fn new(paths: impl IntoIterator<Item = String>) -> Self {
        let mut set = HashSet::new();
        for path in paths {
            if let Some(components) = normalize_path(Path::new(&path)) {
                set.insert(components);
            }
        }
        Self { paths: set }
    }

    /// 该路径是否落在任一已导入目录之内（含自身）
    pub fn contains(&self, path: &Path) -> bool {
        match normalize_path(path) {
            Some(components) => (1..=components.len())
                .any(|length| self.paths.contains(&components[..length])),
            None => false,
        }
    }
}

/// 执行扫描。阻塞式，调用方应放到 `spawn_blocking` 中。
pub fn scan(opts: &ScanOptions) -> AppResult<Vec<ScanCandidate>> {
    let root = PathBuf::from(&opts.root);
    if !root.is_absolute() {
        return Err(AppError::msg("扫描根目录必须是绝对路径"));
    }
    if !root.is_dir() {
        return Err(AppError::msg(format!(
            "目录不存在或不是文件夹: {}",
            root.display()
        )));
    }

    let max_depth = opts.max_depth.clamp(1, 6);
    let started = Instant::now();

    let candidates = if opts.mode == "first_level" {
        scan_first_level(&root, opts)?
    } else {
        scan_by_executable(&root, max_depth, opts)?
    };

    log::info!(
        "扫描完成 root={} mode={} depth={} 命中={} 耗时={}ms",
        root.display(),
        opts.mode,
        max_depth,
        candidates.len(),
        started.elapsed().as_millis()
    );
    Ok(candidates)
}

/// 模式一：扫描一级子目录，每个子目录都视为一个游戏（即使没有 exe）。
fn scan_first_level(root: &Path, opts: &ScanOptions) -> AppResult<Vec<ScanCandidate>> {
    let mut results = Vec::new();
    let entries = std::fs::read_dir(root)?;

    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if is_excluded_dir(&dir_name) {
            continue;
        }

        let executables = if opts.detect_executables {
            list_executables(&path).unwrap_or_default()
        } else {
            Vec::new()
        };

        results.push(build_candidate(&path, &dir_name, executables, opts));
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// 模式二：递归查找「含有直属可执行文件」的目录，把该目录当成一个游戏。
///
/// 采用「祖先优先短路」策略：一旦某目录已确认含启动项，就不再向下钻取，
/// 这样能避免把 `Game/Game.exe` 和 `Game/bin/Game.exe` 识别成两个游戏。
fn scan_by_executable(
    root: &Path,
    max_depth: usize,
    opts: &ScanOptions,
) -> AppResult<Vec<ScanCandidate>> {
    let mut exe_by_dir: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut dirs_with_exe: HashSet<PathBuf> = HashSet::new();

    let walker = WalkDir::new(root)
        .min_depth(1)
        .max_depth(max_depth)
        .follow_links(false)
        .sort_by(|a, b| b.file_type().is_dir().cmp(&a.file_type().is_dir()))
        .into_iter();

    for entry in walker.filter_entry(|e| {
        // 目录剪枝：排除目录直接不进入
        !(e.file_type().is_dir() && is_excluded_dir(&e.file_name().to_string_lossy()))
    }) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        if entry.file_type().is_dir() {
            // 父目录已有直属 exe → 该子目录无需继续遍历
            if path
                .parent()
                .map(|p| dirs_with_exe.contains(p))
                .unwrap_or(false)
            {
                continue;
            }
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }
        let parent = match path.parent() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        if parent == root {
            continue; // 忽略扫描根目录直属的文件
        }
        if !has_valid_extension(path) || is_excluded_exe(path) {
            continue;
        }

        dirs_with_exe.insert(parent.clone());
        if let Some(name) = path.file_name() {
            exe_by_dir
                .entry(parent)
                .or_default()
                .push(name.to_string_lossy().to_string());
        }
    }

    // 祖先优先：按路径深度升序，若祖先已被选中则丢弃
    let mut dirs: Vec<PathBuf> = exe_by_dir.keys().cloned().collect();
    dirs.sort_by_key(|p| p.components().count());

    let mut selected: Vec<PathBuf> = Vec::new();
    let mut selected_set: HashSet<PathBuf> = HashSet::new();
    for dir in dirs {
        let has_selected_ancestor = dir
            .ancestors()
            .skip(1)
            .any(|ancestor| selected_set.contains(ancestor));
        if !has_selected_ancestor {
            selected_set.insert(dir.clone());
            selected.push(dir);
        }
    }

    let mut results: Vec<ScanCandidate> = Vec::new();
    for game_dir in selected {
        let mut executables = exe_by_dir.remove(&game_dir).unwrap_or_default();
        let raw_name = game_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let name = trim_dirname_to_search_name(&raw_name);
        sort_executables(&mut executables, &name);
        results.push(build_candidate(&game_dir, &raw_name, executables, opts));
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

fn build_candidate(
    path: &Path,
    raw_dir_name: &str,
    executables: Vec<String>,
    opts: &ScanOptions,
) -> ScanCandidate {
    let name = trim_dirname_to_search_name(raw_dir_name);
    let (engine_id, confidence) = if opts.detect_engine {
        match engine::detect(path) {
            Some(info) => (Some(info.id), info.confidence),
            None => (None, 0),
        }
    } else {
        (None, 0)
    };

    ScanCandidate {
        name,
        path: path.to_string_lossy().to_string(),
        executables,
        engine: engine_id,
        engine_confidence: confidence,
        already_imported: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_removes_bracket_tags() {
        assert_eq!(trim_dirname_to_search_name("[1103] 千恋万花"), "千恋万花");
        assert_eq!(trim_dirname_to_search_name("【品牌】游戏名（初回版）"), "游戏名");
        assert_eq!(trim_dirname_to_search_name("A (B) C"), "A C");
    }

    #[test]
    fn trim_falls_back_when_all_removed() {
        assert_eq!(trim_dirname_to_search_name("[社团]"), "[社团]");
    }

    #[test]
    fn excluded_exe_matches_known_noise() {
        assert!(is_excluded_exe(Path::new("unins000.exe")));
        assert!(is_excluded_exe(Path::new("vcredist_x64.exe")));
        assert!(is_excluded_exe(Path::new("打开存档目录.bat")));
        assert!(!is_excluded_exe(Path::new("Game.exe")));
    }

    #[test]
    fn executables_prefer_chinese_and_game_name() {
        let mut list = vec![
            "Game.exe".to_string(),
            "死に逝く君.exe".to_string(),
            "Game_chs.exe".to_string(),
            "游戏.exe".to_string(),
        ];
        sort_executables(&mut list, "Game");
        assert_eq!(
            list,
            ["Game_chs.exe", "游戏.exe", "Game.exe", "死に逝く君.exe"]
        );
    }
}
