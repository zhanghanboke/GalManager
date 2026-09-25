//! 检索索引：把中文标题转成「全拼 + 首字母」，让 `qlwh` 也能搜到「千恋万花」。
//!
//! 生成的索引串形如 `qianlianwanhua qlwh`，配合 SQL 的 `LIKE '%关键词%'` 使用：
//! - 输入 `qlwh`   → 命中首字母部分
//! - 输入 `qianlian` → 命中全拼部分
//! - 输入 `千恋`    → 仍由原有的 title LIKE 命中（索引不替代它）
//!
//! 非汉字（拉丁字母、数字、假名等）原样保留，因此中英混排标题如
//! `Summer Pockets REFLECTION BLUE` 依然能按原文搜到。

use pinyin::ToPinyin;

/// 为一段文本生成「全拼 首字母」检索串（已转小写）。
///
/// 没有任何汉字时两个部分相同，等于把原文小写化，行为可预期。
pub fn pinyin_index(input: &str) -> String {
    let mut full = String::with_capacity(input.len() * 4);
    let mut initials = String::with_capacity(input.len());

    for ch in input.chars() {
        match ch.to_pinyin() {
            Some(pinyin) => {
                full.push_str(pinyin.plain());
                initials.push_str(pinyin.first_letter());
            }
            None => {
                // 空白字符不参与，避免索引里出现大量无意义空格
                if ch.is_whitespace() {
                    continue;
                }
                for lower in ch.to_lowercase() {
                    full.push(lower);
                    initials.push(lower);
                }
            }
        }
    }

    format!("{full} {initials}")
}

/// 拼出一个游戏的完整检索索引：标题 + 原名 + 开发商。
pub fn build_search_index(
    title: &str,
    original_title: Option<&str>,
    developer: Option<&str>,
) -> String {
    let mut parts = vec![pinyin_index(title)];

    if let Some(original) = original_title.filter(|s| !s.trim().is_empty()) {
        parts.push(pinyin_index(original));
    }
    if let Some(developer) = developer.filter(|s| !s.trim().is_empty()) {
        parts.push(pinyin_index(developer));
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一次性工具：打印演示数据的真实拼音索引，供 `scripts/mock.html` 使用。
    /// 需要时用 `cargo test --lib -- --ignored --nocapture print_mock_indices` 运行。
    #[test]
    #[ignore]
    fn print_mock_indices() {
        let rows = [
            ("千恋万花", "柚子社"),
            ("ATRI -My Dear Moments-", "Frontwing"),
            ("樱之诗", "枕"),
            ("Summer Pockets REFLECTION BLUE", "Key"),
            ("素晴日", "ケロQ"),
            ("苍之彼方的四重奏", "sprite"),
            ("近月少女的礼仪", "Navel"),
            ("美好的每一天", "ケロQ"),
            ("月姬", "TYPE-MOON"),
            ("Fate/stay night", "TYPE-MOON"),
            ("白色相簿2", "Leaf"),
            ("ISLAND", "Frontwing"),
            ("纸上的魔法使", "ういんどみるOasis"),
            ("金辉恋曲四重奏", "SAGA PLANETS"),
            ("爱上火车", "Lose"),
            ("CLANNAD", "Key"),
        ];
        for (title, developer) in rows {
            let index = build_search_index(title, None, Some(developer));
            println!("MOCK\t{title}\t{index}");
        }
    }

    #[test]
    fn chinese_title_produces_full_and_initials() {
        let index = pinyin_index("千恋万花");
        assert!(index.contains("qianlianwanhua"), "全拼缺失: {index}");
        assert!(index.contains("qlwh"), "首字母缺失: {index}");
    }

    #[test]
    fn initials_are_searchable_by_prefix() {
        // 用户实际输入的就是首字母，验证能被 LIKE '%qlwh%' 命中
        let index = pinyin_index("千恋万花");
        assert!(index.contains("qlwh"));
    }

    #[test]
    fn latin_text_is_lowercased_and_kept() {
        let index = pinyin_index("Summer Pockets");
        assert!(index.contains("summerpockets"), "{index}");
    }

    #[test]
    fn mixed_title_keeps_both_sides() {
        let index = pinyin_index("ATRI -My Dear Moments-");
        assert!(index.contains("atri"), "{index}");
        assert!(index.contains("mydearmoments"), "{index}");
    }

    #[test]
    fn japanese_kana_is_kept_verbatim() {
        // 假名没有拼音，应原样保留，方便按日文原名搜索
        let index = pinyin_index("まいてつ");
        assert!(index.contains("まいてつ"), "{index}");
    }

    #[test]
    fn whitespace_is_skipped() {
        let index = pinyin_index(" 千 恋 ");
        assert!(index.contains("qianlian"), "{index}");
        assert!(!index.starts_with(' '), "不应有前导空格: {index}");
    }

    #[test]
    fn empty_input_is_safe() {
        assert_eq!(pinyin_index(""), " ");
    }

    #[test]
    fn build_index_includes_developer_and_original_title() {
        let index = build_search_index("千恋万花", Some("千恋＊万花"), Some("柚子社"));
        assert!(index.contains("qlwh"), "标题首字母缺失: {index}");
        assert!(index.contains("youzishe"), "开发商全拼缺失: {index}");
        assert!(index.contains("yzs"), "开发商首字母缺失: {index}");
    }

    #[test]
    fn build_index_skips_blank_optional_fields() {
        let index = build_search_index("千恋万花", Some("   "), None);
        assert!(index.contains("qlwh"));
        // 只有一段，不应有多余的分隔
        assert_eq!(index.matches(' ').count(), 1, "{index}");
    }

    /// 反向验证：首字母检索不应把无关标题也匹配进来
    #[test]
    fn unrelated_title_is_not_matched() {
        let index = pinyin_index("白色相簿2");
        assert!(!index.contains("qlwh"), "{index}");
    }
}
