//! 命名规则展开：库名模板笛卡尔积 → 物理库名列表

use crate::models::{NamingRule, PartKind};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Seg {
    Lit(String),
    Part(PartKind),
}

fn placeholder_zh(kind: PartKind) -> &'static str {
    match kind {
        PartKind::Tenant => "{租户}",
        PartKind::Year => "{年份}",
        PartKind::Shard => "{分片}",
    }
}

fn parse_placeholder(inner: &str) -> Option<PartKind> {
    let raw = inner.trim();
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "year" | "年份" => Some(PartKind::Year),
        "tenant" | "租户" => Some(PartKind::Tenant),
        "shard" | "分片" => Some(PartKind::Shard),
        _ => None,
    }
}

fn parse_pattern(pattern: &str) -> Vec<Seg> {
    let mut segs = Vec::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    let mut lit = String::new();
    while i < chars.len() {
        if chars[i] == '{' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '}') {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                if let Some(kind) = parse_placeholder(&inner) {
                    if !lit.is_empty() {
                        segs.push(Seg::Lit(std::mem::take(&mut lit)));
                    }
                    segs.push(Seg::Part(kind));
                    i += end + 2;
                    continue;
                }
            }
        }
        lit.push(chars[i]);
        i += 1;
    }
    if !lit.is_empty() {
        segs.push(Seg::Lit(lit));
    }
    segs
}

fn values_for<'a>(rule: &'a NamingRule, kind: PartKind) -> &'a [String] {
    match kind {
        PartKind::Tenant => &rule.tenants,
        PartKind::Year => &rule.years,
        PartKind::Shard => &rule.shards,
    }
}

/// 由旧字段推导模板：`logical_name` + `_` + 占位符
pub fn legacy_pattern(rule: &NamingRule) -> String {
    let mut p = rule.logical_name.clone();
    for kind in &rule.parts_order {
        if !p.is_empty() {
            p.push('_');
        }
        p.push_str(placeholder_zh(*kind));
    }
    p
}

/// 实际用于展开的模板（优先 pattern）
pub fn effective_pattern(rule: &NamingRule) -> String {
    let trimmed = rule.pattern.trim();
    if !trimmed.is_empty() {
        trimmed.to_string()
    } else {
        legacy_pattern(rule)
    }
}

/// 模板中按出现顺序的占位符种类
pub fn placeholders_in_pattern(pattern: &str) -> Vec<PartKind> {
    parse_pattern(pattern)
        .into_iter()
        .filter_map(|s| match s {
            Seg::Part(k) => Some(k),
            Seg::Lit(_) => None,
        })
        .collect()
}

/// 规范化规则：补齐 pattern / display_name，并同步 parts_order
pub fn normalize_rule(mut rule: NamingRule) -> NamingRule {
    if rule.pattern.trim().is_empty() {
        rule.pattern = legacy_pattern(&rule);
    } else {
        rule.pattern = rule.pattern.trim().to_string();
    }
    rule.parts_order = placeholders_in_pattern(&rule.pattern);
    // 兼容：logical_name 取模板开头字面量（去掉尾部分隔符）
    if let Some(Seg::Lit(lit)) = parse_pattern(&rule.pattern).into_iter().next() {
        let trimmed = lit.trim_end_matches(['_', '-', '.']).to_string();
        if !trimmed.is_empty() {
            rule.logical_name = trimmed;
        }
    } else if rule.logical_name.is_empty() && !rule.pattern.is_empty() {
        rule.logical_name = rule.pattern.clone();
    }
    if rule.display_name.trim().is_empty() {
        rule.display_name = rule.pattern.clone();
    } else {
        rule.display_name = rule.display_name.trim().to_string();
    }
    rule
}

/// 按库名模板笛卡尔积展开物理库名
pub fn expand_database_names(rule: &NamingRule) -> Vec<String> {
    let pattern = effective_pattern(rule);
    if pattern.is_empty() {
        return Vec::new();
    }
    let segs = parse_pattern(&pattern);
    let part_kinds: Vec<PartKind> = segs
        .iter()
        .filter_map(|s| match s {
            Seg::Part(k) => Some(*k),
            Seg::Lit(_) => None,
        })
        .collect();

    if part_kinds.is_empty() {
        let name: String = segs
            .iter()
            .map(|s| match s {
                Seg::Lit(t) => t.as_str(),
                Seg::Part(_) => "",
            })
            .collect();
        return if name.is_empty() {
            Vec::new()
        } else {
            vec![name]
        };
    }

    let mut combos: Vec<Vec<&str>> = vec![vec![]];
    for kind in &part_kinds {
        let values = values_for(rule, *kind);
        if values.is_empty() {
            return Vec::new();
        }
        let mut next = Vec::with_capacity(combos.len() * values.len());
        for combo in &combos {
            for v in values {
                let mut c = combo.clone();
                c.push(v.as_str());
                next.push(c);
            }
        }
        combos = next;
    }

    combos
        .into_iter()
        .map(|combo| {
            let mut i = 0usize;
            let mut out = String::new();
            for seg in &segs {
                match seg {
                    Seg::Lit(t) => out.push_str(t),
                    Seg::Part(_) => {
                        out.push_str(combo[i]);
                        i += 1;
                    }
                }
            }
            out
        })
        .collect()
}

/// 用模板反匹配库名，成功则返回各占位符捕获值
pub fn match_pattern_captures(
    pattern: &str,
    database: &str,
) -> Option<Vec<(PartKind, String)>> {
    let segs = parse_pattern(pattern.trim());
    if segs.is_empty() {
        return None;
    }
    match_segs(&segs, database)
}

fn match_segs(segs: &[Seg], input: &str) -> Option<Vec<(PartKind, String)>> {
    if segs.is_empty() {
        return if input.is_empty() {
            Some(Vec::new())
        } else {
            None
        };
    }
    match &segs[0] {
        Seg::Lit(lit) => {
            let rest = input.strip_prefix(lit.as_str())?;
            match_segs(&segs[1..], rest)
        }
        Seg::Part(kind) => {
            if segs.len() == 1 {
                if input.is_empty() {
                    return None;
                }
                return Some(vec![(*kind, input.to_string())]);
            }
            // 按字符边界尝试捕获长度（由短到长），保证 UTF-8 安全
            let mut ends: Vec<usize> = input.char_indices().map(|(i, _)| i).collect();
            ends.push(input.len());
            for &end in ends.iter().skip(1) {
                let captured = &input[..end];
                if captured.is_empty() {
                    continue;
                }
                if let Some(mut rest) = match_segs(&segs[1..], &input[end..]) {
                    let mut out = vec![(*kind, captured.to_string())];
                    out.append(&mut rest);
                    return Some(out);
                }
            }
            None
        }
    }
}

/// 规则对某库名的匹配分；不匹配返回 None
pub fn score_rule_for_database(rule: &NamingRule, database: &str) -> Option<u32> {
    let pattern = effective_pattern(rule);
    if pattern.is_empty() {
        return None;
    }
    let caps = match_pattern_captures(&pattern, database)?;
    let lit_len: u32 = parse_pattern(&pattern)
        .iter()
        .map(|s| match s {
            Seg::Lit(t) => t.chars().count() as u32,
            Seg::Part(_) => 0,
        })
        .sum();
    let mut score = 50 + lit_len;
    for (kind, val) in caps {
        let values = values_for(rule, kind);
        if values.iter().any(|v| v == &val) {
            score += 30;
        } else if values.is_empty() {
            score += 8;
        } else {
            score += 4;
        }
    }
    Some(score)
}

/// 为基准库挑选最佳规则；(规则, 命中总数)
pub fn suggest_best_rule<'a>(
    rules: &'a [NamingRule],
    database: &str,
) -> (Option<&'a NamingRule>, usize) {
    let mut scored: Vec<(u32, &NamingRule)> = rules
        .iter()
        .filter_map(|r| score_rule_for_database(r, database).map(|s| (s, r)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    let count = scored.len();
    (scored.into_iter().next().map(|(_, r)| r), count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NamingRule;

    #[test]
    fn only_literal_pattern() {
        let r = NamingRule {
            pattern: "order".into(),
            ..Default::default()
        };
        assert_eq!(expand_database_names(&r), vec!["order"]);
    }

    #[test]
    fn pattern_year_tenant() {
        let r = NamingRule {
            pattern: "order_{年份}_{租户}".into(),
            years: vec!["2025".into(), "2026".into()],
            tenants: vec!["demo".into()],
            ..Default::default()
        };
        assert_eq!(
            expand_database_names(&r),
            vec![
                "order_2025_demo".to_string(),
                "order_2026_demo".to_string()
            ]
        );
    }

    #[test]
    fn custom_separator_in_pattern() {
        let r = NamingRule {
            pattern: "order-{year}-{tenant}".into(),
            years: vec!["2025".into()],
            tenants: vec!["demo".into()],
            ..Default::default()
        };
        assert_eq!(expand_database_names(&r), vec!["order-2025-demo"]);
    }

    #[test]
    fn legacy_logical_and_parts_order() {
        let r = NamingRule {
            logical_name: "order".into(),
            parts_order: vec![PartKind::Tenant, PartKind::Year, PartKind::Shard],
            tenants: vec!["demo".into()],
            years: vec!["2025".into()],
            shards: vec!["1".into()],
            ..Default::default()
        };
        assert_eq!(expand_database_names(&r), vec!["order_demo_2025_1"]);
        let n = normalize_rule(r);
        assert_eq!(n.pattern, "order_{租户}_{年份}_{分片}");
        assert_eq!(n.display_name, "order_{租户}_{年份}_{分片}");
    }

    #[test]
    fn shard_only_legacy() {
        let r = NamingRule {
            logical_name: "order".into(),
            parts_order: vec![PartKind::Shard],
            shards: vec!["1".into(), "2".into()],
            ..Default::default()
        };
        assert_eq!(
            expand_database_names(&r),
            vec!["order_1".to_string(), "order_2".to_string()]
        );
    }

    #[test]
    fn empty_values_yield_empty() {
        let r = NamingRule {
            pattern: "order_{年份}".into(),
            years: vec![],
            ..Default::default()
        };
        assert!(expand_database_names(&r).is_empty());
    }

    #[test]
    fn reverse_match_year_tenant() {
        let caps =
            match_pattern_captures("order_{年份}_{租户}", "order_2025_demo").unwrap();
        assert_eq!(caps[0], (PartKind::Year, "2025".into()));
        assert_eq!(caps[1], (PartKind::Tenant, "demo".into()));
    }

    #[test]
    fn suggest_prefers_value_list_hit() {
        let rules = vec![
            NamingRule {
                id: "a".into(),
                pattern: "order_{年份}_{租户}".into(),
                years: vec!["2024".into()],
                tenants: vec!["other".into()],
                ..Default::default()
            },
            NamingRule {
                id: "b".into(),
                pattern: "order_{年份}_{租户}".into(),
                years: vec!["2025".into()],
                tenants: vec!["demo".into()],
                ..Default::default()
            },
        ];
        let (best, count) = suggest_best_rule(&rules, "order_2025_demo");
        assert_eq!(count, 2);
        assert_eq!(best.unwrap().id, "b");
    }
}
