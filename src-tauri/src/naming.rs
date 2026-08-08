//! 命名规则展开：将可组合规则转为物理库名列表

use crate::models::{NamingRule, PartKind};

/// 按 `parts_order` 笛卡尔积展开物理库名；部件间以 `_` 连接，前缀为 `logical_name`。
pub fn expand_database_names(rule: &NamingRule) -> Vec<String> {
    if rule.parts_order.is_empty() {
        return vec![rule.logical_name.clone()];
    }

    let mut dimensions = Vec::with_capacity(rule.parts_order.len());
    for kind in &rule.parts_order {
        let values = match kind {
            PartKind::Tenant => &rule.tenants,
            PartKind::Year => &rule.years,
            PartKind::Shard => &rule.shards,
        };
        if values.is_empty() {
            return vec![];
        }
        dimensions.push(values);
    }

    let mut names = vec![rule.logical_name.clone()];
    for values in dimensions {
        let mut next = Vec::with_capacity(names.len() * values.len());
        for prefix in &names {
            for value in values {
                next.push(format!("{prefix}_{value}"));
            }
        }
        names = next;
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NamingRule;

    #[test]
    fn only_logical_name() {
        let r = NamingRule {
            logical_name: "order".into(),
            parts_order: vec![],
            tenants: vec![],
            years: vec![],
            shards: vec![],
            ..Default::default()
        };
        assert_eq!(expand_database_names(&r), vec!["order"]);
    }

    #[test]
    fn tenant_year_shard_order() {
        let r = NamingRule {
            logical_name: "order".into(),
            parts_order: vec![PartKind::Tenant, PartKind::Year, PartKind::Shard],
            tenants: vec!["lemi".into()],
            years: vec!["2025".into()],
            shards: vec!["1".into()],
            ..Default::default()
        };
        assert_eq!(expand_database_names(&r), vec!["order_lemi_2025_1"]);
    }

    #[test]
    fn shard_only() {
        let r = NamingRule {
            logical_name: "order".into(),
            parts_order: vec![PartKind::Shard],
            tenants: vec![],
            years: vec![],
            shards: vec!["1".into(), "2".into()],
            ..Default::default()
        };
        assert_eq!(
            expand_database_names(&r),
            vec!["order_1".to_string(), "order_2".to_string()]
        );
    }
}
