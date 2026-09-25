//! 全局数据模型定义。
//!
//! 所有结构体同时作为「数据库行」与「前后端传输对象」使用，
//! 统一用 `serde` 的 camelCase 输出，前端可直接消费。

use serde::{Deserialize, Serialize};

/// 游玩状态。用字符串存库，便于后续扩展而无需迁移。
pub const STATUS_UNPLAYED: &str = "unplayed";
pub const STATUS_PLAYING: &str = "playing";
pub const STATUS_COMPLETED: &str = "completed";
pub const STATUS_ON_HOLD: &str = "on_hold";
pub const STATUS_DROPPED: &str = "dropped";

/// 全部合法游玩状态，用于入参校验。
pub fn all_statuses() -> [&'static str; 5] {
    [
        STATUS_UNPLAYED,
        STATUS_PLAYING,
        STATUS_COMPLETED,
        STATUS_ON_HOLD,
        STATUS_DROPPED,
    ]
}

/// 校验游玩状态是否合法。
pub fn is_valid_status(value: &str) -> bool {
    all_statuses().contains(&value)
}

/// 分类（侧边栏一级分组）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub sort_order: i64,
    /// 该分类下的游戏数量（查询时聚合，写入时忽略）
    #[serde(default)]
    pub game_count: i64,
}

/// 标签（多对多）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    #[serde(default)]
    pub game_count: i64,
}

/// 游戏主记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: i64,
    pub title: String,
    /// 原名（日文/英文），用于搜索与排序
    pub original_title: Option<String>,
    /// 游戏根目录绝对路径
    pub path: Option<String>,
    /// 启动程序，相对 `path` 的路径
    pub executable: Option<String>,
    /// 额外启动参数
    pub args: Option<String>,
    /// 封面本地缓存路径
    pub cover_path: Option<String>,
    /// 引擎标识（kirikiri / renpy / unity / rpgmaker ...）
    pub engine: Option<String>,
    /// 引擎识别置信度 0-100
    pub engine_confidence: i64,
    pub category_id: Option<i64>,
    pub play_status: String,
    pub favorite: i64,
    /// 0-100，-1 表示未评分
    pub rating: i64,
    /// 是否通过 Locale Emulator 转区启动
    pub le_launch: i64,
    /// 转区区域，如 ja_JP
    pub le_locale: Option<String>,
    pub total_play_seconds: i64,
    pub last_played_at: Option<String>,
    pub release_date: Option<String>,
    pub developer: Option<String>,
    pub description: Option<String>,
    /// 备注
    pub notes: Option<String>,
    /// 手动指定的存档目录（覆盖自动识别结果）
    pub save_path: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,

    // ---- 以下为查询时聚合出来的字段，非表列 ----
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub session_count: i64,
    #[serde(default)]
    pub save_count: i64,
}

/// 新增/更新游戏的入参
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInput {
    pub title: String,
    #[serde(default)]
    pub original_title: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub args: Option<String>,
    #[serde(default)]
    pub cover_path: Option<String>,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub engine_confidence: Option<i64>,
    #[serde(default)]
    pub category_id: Option<i64>,
    #[serde(default)]
    pub play_status: Option<String>,
    #[serde(default)]
    pub favorite: Option<i64>,
    #[serde(default)]
    pub rating: Option<i64>,
    #[serde(default)]
    pub le_launch: Option<i64>,
    #[serde(default)]
    pub le_locale: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub developer: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub save_path: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// 游戏库筛选条件
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFilter {
    /// 关键词（标题 / 原名 / 开发商，支持拼音首字母）
    #[serde(default)]
    pub keyword: String,
    /// 分类 ID，None = 全部
    #[serde(default)]
    pub category_id: Option<i64>,
    /// 标签名列表（AND 关系）
    #[serde(default)]
    pub tags: Vec<String>,
    /// 游玩状态过滤
    #[serde(default)]
    pub statuses: Vec<String>,
    /// 仅收藏
    #[serde(default)]
    pub favorite_only: bool,
    /// 引擎过滤
    #[serde(default)]
    pub engines: Vec<String>,
    /// 排序字段：title / last_played / playtime / created / rating
    #[serde(default)]
    pub sort_by: Option<String>,
    /// 是否倒序
    #[serde(default)]
    pub sort_desc: bool,
}

/// 扫描时发现的「可能重复」。
///
/// 与 `already_imported`（路径归一化后落在已入库目录内）不同，
/// 这里描述的是**路径不同但标题一致**的情况：可能是同一个游戏被换了目录/换了盘，
/// 也可能是有意分开的两份。因此只作为提示，不阻止导入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateHint {
    /// 库中那条游戏的 id
    pub game_id: i64,
    pub title: String,
    /// 库中那条游戏的目录，供用户核对是否真的是同一个
    pub path: Option<String>,
}

/// 扫描得到的候选游戏
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCandidate {
    /// 目录名清洗后的游戏名
    pub name: String,
    /// 目录绝对路径
    pub path: String,
    /// 可执行文件（相对目录）
    pub executables: Vec<String>,
    /// 识别到的引擎
    pub engine: Option<String>,
    pub engine_confidence: i64,
    /// 是否已在库中
    pub already_imported: bool,
    /// 路径不同但疑似同一个游戏时的提示
    #[serde(default)]
    pub possible_duplicate: Option<DuplicateHint>,
}

/// 扫描入参
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    pub root: String,
    /// 最大递归深度（1-6）
    pub max_depth: usize,
    /// 扫描模式：executable（按含 exe 的目录）| first_level（仅一级子目录）
    pub mode: String,
    /// 是否解析可执行文件列表
    #[serde(default = "default_true")]
    pub detect_executables: bool,
    /// 是否识别引擎
    #[serde(default = "default_true")]
    pub detect_engine: bool,
}

fn default_true() -> bool {
    true
}

/// 游玩会话
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySession {
    pub id: i64,
    pub game_id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: i64,
    pub note: Option<String>,
    /// 冗余字段，便于时间线直接渲染
    #[serde(default)]
    pub game_title: Option<String>,
}

/// 存档槽位
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSlot {
    pub id: i64,
    pub game_id: i64,
    pub slot_name: String,
    pub engine: Option<String>,
    /// 备份来源目录
    pub source_path: String,
    /// 归档文件路径
    pub backup_path: String,
    pub size_bytes: i64,
    pub file_count: i64,
    pub remark: Option<String>,
    pub created_at: String,
}

/// 存档路径探测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePathProbe {
    pub engine: String,
    pub engine_label: String,
    pub confidence: i64,
    /// 探测到的候选目录（存在且非空）
    pub paths: Vec<SavePathCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePathCandidate {
    pub path: String,
    pub exists: bool,
    pub file_count: i64,
    pub size_bytes: i64,
    pub source: String,
}

/// 汉化补丁 / 附加资源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    pub id: i64,
    pub game_id: i64,
    pub name: String,
    pub version: Option<String>,
    /// translation / uncensor / crack / other
    pub patch_type: String,
    pub file_path: Option<String>,
    pub url: Option<String>,
    pub installed: i64,
    pub remark: Option<String>,
    pub created_at: String,
}

/// 攻略笔记
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    pub game_id: i64,
    pub title: String,
    pub content: String,
    pub updated_at: String,
    pub created_at: String,
}

/// 资源链接收藏
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLink {
    pub id: i64,
    pub game_id: Option<i64>,
    pub title: String,
    pub url: String,
    /// wiki / patch / video / forum / other
    pub kind: String,
    pub remark: Option<String>,
    pub created_at: String,
}

/// 全局统计概览
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsOverview {
    pub total_games: i64,
    pub total_play_seconds: i64,
    pub completed_games: i64,
    pub playing_games: i64,
    pub favorite_games: i64,
    /// 近 7 天游玩时长
    pub week_play_seconds: i64,
    /// 近 30 天游玩时长
    pub month_play_seconds: i64,
    /// 已备份存档数
    pub save_slot_count: i64,
}

/// 单日游玩数据（时间线 / 热力图）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyPlaytime {
    pub date: String,
    pub seconds: i64,
    pub sessions: i64,
}

/// 游戏维度排行
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePlaytimeRank {
    pub game_id: i64,
    pub title: String,
    pub cover_path: Option<String>,
    pub seconds: i64,
    pub sessions: i64,
}

/// 年度游玩报告
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YearlyReport {
    pub year: i32,
    pub total_seconds: i64,
    pub total_sessions: i64,
    pub active_days: i64,
    /// 日均时长（仅统计有游玩的日子）
    pub average_seconds: i64,
    pub longest_session_seconds: i64,
    pub longest_session_game: Option<String>,
    /// 最常游玩的游戏
    pub top_game: Option<String>,
    /// 当年新通关数量
    pub completed_count: i64,
    /// 当年新增入库数量
    pub added_count: i64,
    /// 12 个月的时长分布
    pub monthly: Vec<i64>,
    /// 按星期几的分布（0=周一）
    pub weekday: Vec<i64>,
    /// 每日时长
    pub daily: Vec<DailyPlaytime>,
    /// 游戏排行
    pub ranking: Vec<GamePlaytimeRank>,
}

/// 引擎探测信息（详情页展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub id: String,
    pub label: String,
    pub confidence: i64,
    pub evidence: Vec<String>,
}
