//! Galgame 引擎识别。
//!
//! 识别方式为「特征文件 / 特征目录 / 可执行文件名」多信号加权打分。
//! 之所以不用魔数解析 exe，是因为 Galgame 大量使用壳与保护，
//! 读取 PE 段成本高且收益有限；文件名 + 目录结构已经足够可靠。

use crate::models::EngineInfo;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 引擎静态描述
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDescriptor {
    pub id: String,
    pub label: String,
    /// 默认存档相对位置说明，用于 UI 提示
    pub save_hint: String,
}

/// 全部受支持引擎的元数据
pub const ENGINES: &[(&str, &str, &str)] = &[
    ("kirikiri", "吉里吉里 / KAG", "%APPDATA%\\<厂商>\\<游戏> 或游戏目录 savedata"),
    ("renpy", "Ren'Py", "%APPDATA%\\RenPy\\<游戏>"),
    ("unity", "Unity", "%USERPROFILE%\\AppData\\LocalLow\\<公司>\\<产品>"),
    ("rpgmaker", "RPG Maker (XP/VX/VA)", "游戏目录 Save*.rvdata"),
    ("rpgmaker_mv", "RPG Maker MV/MZ", "www\\save 或 %LOCALAPPDATA%\\<游戏>"),
    ("nscripter", "NScripter", "游戏目录 save*.dat"),
    ("siglus", "SiglusEngine", "%APPDATA%\\<游戏> 或游戏目录 savedata"),
    ("reallive", "RealLive", "游戏目录 save*.sav"),
    ("bgi", "BGI / Ethornell", "游戏目录 save 或 %APPDATA%\\<游戏>"),
    ("artemis", "Artemis Engine", "游戏目录 savedata"),
    ("catystem", "CatSystem2", "游戏目录 save 或 %APPDATA%\\<游戏>"),
    ("wolfrpg", "WOLF RPG Editor", "游戏目录 Data\\SaveData"),
    ("majiro", "Majiro", "游戏目录 save"),
    ("systemnnn", "SystemNNN", "游戏目录 save"),
    ("unreal", "Unreal Engine", "%LOCALAPPDATA%\\<游戏>\\Saved"),
    ("other", "未知 / 其他", "需手动指定"),
];

/// 特征规则：命中即加分
struct Rule {
    /// 目录中存在的文件名（小写，支持后缀匹配）
    files: &'static [&'static str],
    /// 目录中存在的子目录名（小写）
    dirs: &'static [&'static str],
    /// 可执行文件名包含的关键字（小写）
    exe_keywords: &'static [&'static str],
    weight: i32,
}

/// 各引擎的识别规则表
fn rules_for(engine_id: &str) -> Vec<Rule> {
    match engine_id {
        "kirikiri" => vec![
            Rule {
                files: &[".xp3"],
                dirs: &[],
                exe_keywords: &[],
                weight: 45,
            },
            Rule {
                files: &[],
                dirs: &["savedata"],
                exe_keywords: &["krkr", "kiriki", "tvp"],
                weight: 30,
            },
            Rule {
                files: &["kirikiri2.dll", "krkr2.dll", "kag.dll", "tvp(kirikiri)"],
                dirs: &[],
                exe_keywords: &[],
                weight: 35,
            },
        ],
        "renpy" => vec![
            Rule {
                files: &[".rpa", ".rpy", ".rpyc"],
                dirs: &[],
                exe_keywords: &[],
                weight: 45,
            },
            Rule {
                files: &[],
                dirs: &["renpy", "game"],
                exe_keywords: &["renpy"],
                weight: 30,
            },
            Rule {
                files: &["renpy.exe"],
                dirs: &[],
                exe_keywords: &[],
                weight: 40,
            },
        ],
        "unity" => vec![
            Rule {
                files: &["unityplayer.dll"],
                dirs: &[],
                exe_keywords: &[],
                weight: 50,
            },
            Rule {
                files: &["globalgamemanagers", "app.info"],
                dirs: &[],
                exe_keywords: &[],
                weight: 35,
            },
            Rule {
                files: &[],
                dirs: &[],
                exe_keywords: &["_data", "mono"],
                weight: 15,
            },
        ],
        "rpgmaker" => vec![
            Rule {
                files: &[".rxproj", ".rvproj", ".rvproj2", ".rxdata", ".rvdata", ".rvdata2"],
                dirs: &[],
                exe_keywords: &[],
                weight: 50,
            },
            Rule {
                files: &["rgss102e.dll", "rgss200j.dll", "rgss300.dll", "game.ini"],
                dirs: &[],
                exe_keywords: &[],
                weight: 35,
            },
        ],
        "rpgmaker_mv" => vec![
            Rule {
                files: &["nw.dll", "package.json"],
                dirs: &["www", "js"],
                exe_keywords: &[],
                weight: 40,
            },
            Rule {
                files: &["rpg_core.js", "rpg_windows.js", "main.js"],
                dirs: &[],
                exe_keywords: &[],
                weight: 40,
            },
        ],
        "nscripter" => vec![Rule {
            files: &["nscript.dat", "arc.nsa", "nscr_sec.dat", "nsaarc"],
            dirs: &[],
            exe_keywords: &[],
            weight: 50,
        }],
        "siglus" => vec![
            Rule {
                files: &["scene.pck", "siglusengine.exe"],
                dirs: &[],
                exe_keywords: &["siglus"],
                weight: 50,
            },
            Rule {
                files: &[".pck"],
                dirs: &[],
                exe_keywords: &[],
                weight: 25,
            },
        ],
        "reallive" => vec![Rule {
            files: &["seen.txt", "reallive.exe", ".g00"],
            dirs: &[],
            exe_keywords: &["reallive"],
            weight: 50,
        }],
        "bgi" => vec![Rule {
            files: &[".arc", "bgi.exe", "buriko"],
            dirs: &[],
            exe_keywords: &["bgi", "ethornell"],
            weight: 45,
        }],
        "artemis" => vec![Rule {
            files: &[".pfs", ".afa", ".ald"],
            dirs: &[],
            exe_keywords: &["artemis"],
            weight: 50,
        }],
        "catystem" => vec![Rule {
            files: &[".cst", "cs2conf.dll", "cs2.exe"],
            dirs: &[],
            exe_keywords: &["cs2", "catystem"],
            weight: 45,
        }],
        "wolfrpg" => vec![Rule {
            files: &["data.wolf", "wolfrpg.exe", "game.dat"],
            dirs: &["savedata"],
            exe_keywords: &["wolf"],
            weight: 45,
        }],
        "majiro" => vec![Rule {
            files: &[".mjo", "majiro.exe"],
            dirs: &[],
            exe_keywords: &["majiro"],
            weight: 50,
        }],
        "systemnnn" => vec![Rule {
            files: &["systemnnn.exe", ".nnn"],
            dirs: &[],
            exe_keywords: &["systemnnn", "system4"],
            weight: 50,
        }],
        "unreal" => vec![
            Rule {
                files: &[".pak", ".utoc", ".ucas"],
                dirs: &["engine", "binaries"],
                exe_keywords: &[],
                weight: 45,
            },
            Rule {
                files: &["unrealengine"],
                dirs: &[],
                exe_keywords: &[],
                weight: 40,
            },
        ],
        _ => vec![],
    }
}

/// 目录浅层快照（只取前两层，控制扫描开销）
struct DirSnapshot {
    /// 小写文件名集合
    files: Vec<String>,
    /// 小写目录名集合
    dirs: Vec<String>,
}

fn snapshot(root: &Path) -> DirSnapshot {
    let mut files = Vec::new();
    let mut dirs = Vec::new();

    let read = |dir: &Path, files: &mut Vec<String>, dirs: &mut Vec<String>, collect_dirs: bool| {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                match entry.file_type() {
                    Ok(t) if t.is_dir() => {
                        if collect_dirs {
                            dirs.push(name);
                        }
                    }
                    Ok(t) if t.is_file() => files.push(name),
                    _ => {}
                }
            }
        }
    };

    read(root, &mut files, &mut dirs, true);

    // 再往下一层：游戏主程序常在 bin/ 或 <Game>_Data/ 里
    let subdirs: Vec<String> = dirs.iter().take(24).cloned().collect();
    for sub in subdirs {
        read(&root.join(&sub), &mut files, &mut dirs, false);
    }

    DirSnapshot { files, dirs }
}

impl DirSnapshot {
    fn has_file(&self, token: &str) -> bool {
        if token.starts_with('.') {
            // 后缀特征
            self.files.iter().any(|f| f.ends_with(token))
        } else {
            self.files.iter().any(|f| f == token || f.starts_with(token))
        }
    }

    fn has_dir(&self, token: &str) -> bool {
        self.dirs.iter().any(|d| d == token)
    }

    fn has_exe_keyword(&self, keyword: &str) -> bool {
        self.files
            .iter()
            .filter(|f| f.ends_with(".exe"))
            .any(|f| f.contains(keyword))
    }
}

/// 对单个游戏目录做引擎识别，返回得分最高的结果。
pub fn detect(root: &Path) -> Option<EngineInfo> {
    if !root.is_dir() {
        return None;
    }
    let snap = snapshot(root);
    let mut best: Option<EngineInfo> = None;

    for (id, label, _) in ENGINES {
        if *id == "other" {
            continue;
        }
        let rules = rules_for(id);
        let mut score: i64 = 0;
        let mut evidence: Vec<String> = Vec::new();

        for rule in &rules {
            let mut hit = false;
            for token in rule.files {
                if snap.has_file(token) {
                    evidence.push(format!("存在文件特征 {}", token));
                    hit = true;
                }
            }
            for token in rule.dirs {
                if snap.has_dir(token) {
                    evidence.push(format!("存在目录 {}", token));
                    hit = true;
                }
            }
            for keyword in rule.exe_keywords {
                if snap.has_exe_keyword(keyword) {
                    evidence.push(format!("可执行文件名含 {}", keyword));
                    hit = true;
                }
            }
            if hit {
                score += rule.weight as i64;
            }
        }

        if score > 0 {
            let confidence = score.min(100);
            let is_better = best
                .as_ref()
                .map(|current| confidence > current.confidence)
                .unwrap_or(true);
            if is_better {
                evidence.dedup();
                best = Some(EngineInfo {
                    id: (*id).to_string(),
                    label: (*label).to_string(),
                    confidence,
                    evidence,
                });
            }
        }
    }

    best
}

/// 引擎 id → 中文标签
pub fn label_of(engine_id: &str) -> String {
    ENGINES
        .iter()
        .find(|(id, _, _)| *id == engine_id)
        .map(|(_, label, _)| (*label).to_string())
        .unwrap_or_else(|| engine_id.to_string())
}

/// 引擎列表（供设置页 / 筛选器使用）
pub fn all_engines() -> Vec<EngineDescriptor> {
    ENGINES
        .iter()
        .map(|(id, label, hint)| EngineDescriptor {
            id: (*id).to_string(),
            label: (*label).to_string(),
            save_hint: (*hint).to_string(),
        })
        .collect()
}
