//! 应用数据目录管理。
//!
//! 目录结构：
//! ```text
//! %APPDATA%/com.galmanager.app/
//! ├── galmanager.db      # SQLite 数据库
//! ├── covers/            # 封面图缓存
//! ├── saves/             # 存档备份归档
//! └── logs/              # 运行日志
//! ```

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// 应用数据根目录（由 Tauri 注入，运行期只读）
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
}

impl AppPaths {
    pub fn new(root: PathBuf) -> AppResult<Self> {
        std::fs::create_dir_all(&root)?;
        let paths = Self { root };
        std::fs::create_dir_all(paths.covers_dir())?;
        std::fs::create_dir_all(paths.default_saves_dir())?;
        std::fs::create_dir_all(paths.logs_dir())?;
        Ok(paths)
    }

    pub fn db_file(&self) -> PathBuf {
        self.root.join("galmanager.db")
    }

    pub fn covers_dir(&self) -> PathBuf {
        self.root.join("covers")
    }

    pub fn default_saves_dir(&self) -> PathBuf {
        self.root.join("saves")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }
}

/// 从任意用户输入路径创建目录（用于自定义备份根目录）
pub fn ensure_dir(path: &str) -> AppResult<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("路径不能为空"));
    }
    let dir = PathBuf::from(trimmed);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 清理文件名中的非法字符
pub fn sanitize_file_name(name: &str) -> String {
    const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let cleaned: String = name
        .chars()
        .map(|c| if INVALID.contains(&c) || c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else if trimmed.chars().count() > 80 {
        trimmed.chars().take(80).collect()
    } else {
        trimmed
    }
}

/// 计算文件或目录的大小（字节）
pub fn path_size(path: &Path) -> i64 {
    if path.is_file() {
        return std::fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0);
    }
    walkdir::WalkDir::new(path)
        .max_depth(8)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len() as i64)
        .sum()
}

/// 人类可读的字节数
pub fn human_size(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index < UNITS.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", value, UNITS[index])
    }
}
