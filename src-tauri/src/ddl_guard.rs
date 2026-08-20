//! DDL 投放语句校验：按可配置策略判定常规 / 高风险 / 不允许

use crate::ddl_policy::{DdlPolicy, DdlPolicyLevel, DdlStmtKind};

/// 语句风险：常规 / 高风险（删改数据、删表删字段等）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StmtRisk {
    Normal,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedDdl {
    pub statements: Vec<String>,
    pub risks: Vec<StmtRisk>,
}

impl ValidatedDdl {
    pub fn has_high_risk(&self) -> bool {
        self.risks.iter().any(|r| *r == StmtRisk::High)
    }

    pub fn high_risk_count(&self) -> usize {
        self.risks.iter().filter(|r| **r == StmtRisk::High).count()
    }
}

/// 剥离 SQL 注释：`--` / `#` 行注释、`//` 行注释、`/* */` 块注释。
/// 引号（含反引号）内的内容原样保留。
pub fn strip_sql_comments(sql: &str) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;

    while i < chars.len() {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();

        if in_single {
            out.push(ch);
            if ch == '\\' && next.is_some() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if ch == '\'' {
                // 处理 '' 转义
                if next == Some('\'') {
                    out.push('\'');
                    i += 2;
                    continue;
                }
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            out.push(ch);
            if ch == '\\' && next.is_some() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if ch == '"' {
                if next == Some('"') {
                    out.push('"');
                    i += 2;
                    continue;
                }
                in_double = false;
            }
            i += 1;
            continue;
        }
        if in_backtick {
            out.push(ch);
            if ch == '`' {
                if next == Some('`') {
                    out.push('`');
                    i += 2;
                    continue;
                }
                in_backtick = false;
            }
            i += 1;
            continue;
        }

        // 块注释 /* ... */
        if ch == '/' && next == Some('*') {
            i += 2;
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    i += 2;
                    break;
                }
                i += 1;
            }
            // 用空格占位，避免粘连成新 token
            out.push(' ');
            continue;
        }

        // 行注释 // 或 -- 或 #
        let line_comment = (ch == '/' && next == Some('/'))
            || (ch == '-' && next == Some('-'))
            || ch == '#';
        if line_comment {
            if ch == '/' || ch == '-' {
                i += 2;
            } else {
                i += 1;
            }
            while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
                i += 1;
            }
            out.push(' ');
            continue;
        }

        if ch == '\'' {
            in_single = true;
            out.push(ch);
            i += 1;
            continue;
        }
        if ch == '"' {
            in_double = true;
            out.push(ch);
            i += 1;
            continue;
        }
        if ch == '`' {
            in_backtick = true;
            out.push(ch);
            i += 1;
            continue;
        }

        out.push(ch);
        i += 1;
    }
    out
}

/// 按 `;` 拆分多条语句（忽略引号内的分号；跳过空语句）
pub fn split_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;

    for ch in sql.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' && (in_single || in_double) {
            current.push(ch);
            escape = true;
            continue;
        }
        if ch == '\'' && !in_double && !in_backtick {
            in_single = !in_single;
            current.push(ch);
            continue;
        }
        if ch == '"' && !in_single && !in_backtick {
            in_double = !in_double;
            current.push(ch);
            continue;
        }
        if ch == '`' && !in_single && !in_double {
            in_backtick = !in_backtick;
            current.push(ch);
            continue;
        }
        if ch == ';' && !in_single && !in_double && !in_backtick {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
    out
}

/// 使用默认策略校验（单测与兼容入口）
pub fn validate_executable_ddl(sql: &str) -> Result<ValidatedDdl, String> {
    validate_executable_ddl_with_policy(sql, &DdlPolicy::default())
}

/// 校验可投放 SQL；通过则返回语句列表与逐条风险。
/// 会先剥离注释再拆分；返回的语句为去注释后的可执行文本。
pub fn validate_executable_ddl_with_policy(
    sql: &str,
    policy: &DdlPolicy,
) -> Result<ValidatedDdl, String> {
    let cleaned = strip_sql_comments(sql);
    let statements = split_statements(&cleaned);
    if statements.is_empty() {
        return Err("SQL 为空（或仅含注释）".into());
    }
    let mut risks = Vec::with_capacity(statements.len());
    for (idx, stmt) in statements.iter().enumerate() {
        let risk = classify_statement(stmt, policy)
            .map_err(|e| format!("第 {} 条语句: {e}", idx + 1))?;
        risks.push(risk);
    }
    Ok(ValidatedDdl { statements, risks })
}

/// 兼容旧名：校验并仅返回语句列表
pub fn validate_structure_ddl(sql: &str) -> Result<Vec<String>, String> {
    Ok(validate_executable_ddl(sql)?.statements)
}

fn level_to_risk(level: DdlPolicyLevel, kind_label: &str) -> Result<StmtRisk, String> {
    match level {
        DdlPolicyLevel::Normal => Ok(StmtRisk::Normal),
        DdlPolicyLevel::High => Ok(StmtRisk::High),
        DdlPolicyLevel::Forbidden => Err(format!(
            "当前策略不允许执行「{kind_label}」，可在「设置」中调整"
        )),
    }
}

fn classify_statement(stmt: &str, policy: &DdlPolicy) -> Result<StmtRisk, String> {
    let trimmed = stmt.trim();
    if trimmed.is_empty() {
        return Err("语句为空".into());
    }

    let kinds = detect_stmt_kinds(trimmed)?;
    let level = policy.resolve(&kinds);
    let label = if kinds.len() == 1 {
        kinds[0].label().to_string()
    } else {
        kinds
            .iter()
            .map(|k| k.label())
            .collect::<Vec<_>>()
            .join(" + ")
    };
    level_to_risk(level, &label)
}

/// 识别语句涉及的策略类型（ALTER 可多条）
fn detect_stmt_kinds(stmt: &str) -> Result<Vec<DdlStmtKind>, String> {
    let upper = strip_quoted_literals(stmt).to_ascii_uppercase();
    // 避免列定义 ON UPDATE 干扰；INSERT 的 ON DUPLICATE KEY UPDATE 仍按 INSERT 识别
    let upper_for_head = upper.replace("ON UPDATE", "ON_UPDA_TE");
    let head = upper_for_head.trim_start();

    if starts_with_keyword(head, "DROP DATABASE") || starts_with_keyword(head, "DROP SCHEMA") {
        return Ok(vec![DdlStmtKind::DropDatabase]);
    }
    if starts_with_keyword(head, "INSERT") || starts_with_keyword(head, "REPLACE") {
        return Ok(vec![DdlStmtKind::InsertReplace]);
    }
    if starts_with_keyword(head, "DELETE") {
        return Ok(vec![DdlStmtKind::Delete]);
    }
    if starts_with_keyword(head, "UPDATE") {
        return Ok(vec![DdlStmtKind::Update]);
    }
    if starts_with_keyword(head, "TRUNCATE") {
        return Ok(vec![DdlStmtKind::Truncate]);
    }
    if starts_with_keyword(head, "DROP TABLE") {
        return Ok(vec![DdlStmtKind::DropTable]);
    }
    if starts_with_keyword(head, "DROP INDEX") {
        return Ok(vec![DdlStmtKind::DropIndex]);
    }
    if starts_with_keyword(head, "CREATE UNIQUE INDEX")
        || starts_with_keyword(head, "CREATE INDEX")
    {
        return Ok(vec![DdlStmtKind::CreateIndex]);
    }
    if starts_with_keyword(head, "CREATE TABLE") {
        return Ok(vec![DdlStmtKind::CreateTable]);
    }
    if is_alter_table_head(head) {
        return detect_alter_kinds(stmt);
    }

    Err(
        "未识别的语句类型（当前策略仅覆盖已列出的类型，可在「设置」查看）。\
         支持：CREATE TABLE / ALTER TABLE / CREATE INDEX，以及 INSERT/REPLACE/DROP/DELETE/UPDATE/TRUNCATE 等"
            .into(),
    )
}

/// `ALTER [ONLINE|OFFLINE] [IGNORE] TABLE …`
fn is_alter_table_head(head: &str) -> bool {
    let mut s = head.trim_start();
    if !starts_with_keyword(s, "ALTER") {
        return false;
    }
    s = s["ALTER".len()..].trim_start();
    for _ in 0..6 {
        if starts_with_keyword(s, "ONLINE") {
            s = s["ONLINE".len()..].trim_start();
            continue;
        }
        if starts_with_keyword(s, "OFFLINE") {
            s = s["OFFLINE".len()..].trim_start();
            continue;
        }
        if starts_with_keyword(s, "IGNORE") {
            s = s["IGNORE".len()..].trim_start();
            continue;
        }
        break;
    }
    starts_with_keyword(s, "TABLE")
}

fn detect_alter_kinds(stmt: &str) -> Result<Vec<DdlStmtKind>, String> {
    // 分类只用大写文本；风险关键字匹配不依赖原始大小写
    let upper = stmt.to_ascii_uppercase();
    let mut s = upper.trim_start();
    s = match strip_leading_keyword(s, "ALTER") {
        Some(r) => r.trim_start(),
        None => return Err("无法解析 ALTER TABLE".into()),
    };
    for _ in 0..6 {
        if let Some(r) = strip_leading_keyword(s, "ONLINE") {
            s = r.trim_start();
            continue;
        }
        if let Some(r) = strip_leading_keyword(s, "OFFLINE") {
            s = r.trim_start();
            continue;
        }
        if let Some(r) = strip_leading_keyword(s, "IGNORE") {
            s = r.trim_start();
            continue;
        }
        break;
    }
    s = match strip_leading_keyword(s, "TABLE") {
        Some(r) => r.trim_start(),
        None => return Err("无法解析 ALTER TABLE".into()),
    };
    if s.is_empty() {
        return Err("ALTER TABLE 缺少表名或子句".into());
    }
    let (_table, clauses) = split_table_and_clauses(s)
        .ok_or_else(|| "无法解析 ALTER TABLE 子句".to_string())?;
    if clauses.is_empty() {
        return Err("ALTER TABLE 缺少变更子句".into());
    }

    let mut kinds = Vec::new();
    for clause in split_alter_clauses(clauses) {
        let k = classify_alter_clause_kind(clause.trim())
            .ok_or_else(|| format!("ALTER 子句为空"))?;
        if !kinds.contains(&k) {
            kinds.push(k);
        }
    }
    Ok(kinds)
}

/// 未知子句一律视为常规结构变更；仅明确删除类标为 AlterTableDrop。
fn classify_alter_clause_kind(clause: &str) -> Option<DdlStmtKind> {
    let upper = clause.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return None;
    }
    if is_alter_destructive_clause(&upper) {
        return Some(DdlStmtKind::AlterTableDrop);
    }
    Some(DdlStmtKind::AlterTableSafe)
}

fn is_alter_destructive_clause(upper: &str) -> bool {
    upper.starts_with("DROP COLUMN")
        || upper.starts_with("DROP INDEX")
        || upper.starts_with("DROP KEY")
        || upper.starts_with("DROP PRIMARY KEY")
        || upper.starts_with("DROP FOREIGN KEY")
        || upper.starts_with("DROP CHECK")
        || upper.starts_with("DROP CONSTRAINT")
        || upper.starts_with("DROP PARTITION")
        || upper.starts_with("DISCARD TABLESPACE")
        || upper.starts_with("IMPORT TABLESPACE")
}

fn starts_with_keyword(haystack: &str, keyword: &str) -> bool {
    let h = haystack.trim_start();
    if !h.starts_with(keyword) {
        return false;
    }
    let rest = &h[keyword.len()..];
    rest.is_empty()
        || rest.starts_with(' ')
        || rest.starts_with('\t')
        || rest.starts_with('\n')
        || rest.starts_with('`')
}

fn strip_quoted_literals(stmt: &str) -> String {
    let mut out = String::with_capacity(stmt.len());
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in stmt.chars() {
        if escape {
            if in_single || in_double {
                out.push(' ');
            }
            escape = false;
            continue;
        }
        if ch == '\\' && (in_single || in_double) {
            escape = true;
            continue;
        }
        if ch == '\'' && !in_double {
            in_single = !in_single;
            out.push(' ');
            continue;
        }
        if ch == '"' && !in_single {
            in_double = !in_double;
            out.push(' ');
            continue;
        }
        if in_single || in_double {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

fn strip_leading_keyword<'a>(upper: &'a str, keyword: &str) -> Option<&'a str> {
    if !upper.starts_with(keyword) {
        return None;
    }
    let rest = &upper[keyword.len()..];
    if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') || rest.starts_with('\n') {
        Some(rest)
    } else {
        None
    }
}

fn split_table_and_clauses(rest: &str) -> Option<(&str, &str)> {
    let rest = rest.trim_start();
    let (table_end, _) = read_table_ref(rest)?;
    let table = rest[..table_end].trim();
    let clauses = rest[table_end..].trim_start();
    if clauses.is_empty() {
        return None;
    }
    Some((table, clauses))
}

fn read_table_ref(s: &str) -> Option<(usize, ())> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('`') {
        let end = s[1..].find('`')? + 2;
        return Some((end, ()));
    }
    let end = s
        .find(|c: char| c.is_whitespace())
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((end, ()))
}

fn split_alter_clauses(clauses: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth: i32 = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let bytes = clauses.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        let ch = b as char;
        if ch == '\'' && !in_double && !in_backtick {
            in_single = !in_single;
            continue;
        }
        if ch == '"' && !in_single && !in_backtick {
            in_double = !in_double;
            continue;
        }
        if ch == '`' && !in_single && !in_double {
            in_backtick = !in_backtick;
            continue;
        }
        if in_single || in_double || in_backtick {
            continue;
        }
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
        } else if ch == ',' && depth == 0 {
            parts.push(&clauses[start..i]);
            start = i + 1;
        }
    }
    parts.push(&clauses[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddl_policy::{DdlPolicy, DdlPolicyLevel};

    #[test]
    fn split_statements_handles_semicolons_and_quotes() {
        let sql = "ALTER TABLE t ADD COLUMN c int; ALTER TABLE t2 ADD COLUMN d varchar(10);";
        assert_eq!(split_statements(sql).len(), 2);

        let quoted = "ALTER TABLE t ADD COLUMN c varchar(64) COMMENT 'a;b'";
        let parts = split_statements(quoted);
        assert_eq!(parts.len(), 1);
        assert!(parts[0].contains("'a;b'"));
    }

    #[test]
    fn valid_alter_add_column_passes() {
        let sql = "ALTER TABLE `users` ADD COLUMN `name` varchar(64) NULL COMMENT '姓名';";
        let v = validate_executable_ddl(sql).expect("should pass");
        assert_eq!(v.statements.len(), 1);
        assert_eq!(v.risks[0], StmtRisk::Normal);
    }

    #[test]
    fn valid_alter_modify_column_passes() {
        let sql = "ALTER TABLE users MODIFY COLUMN name varchar(128) NOT NULL;";
        assert!(validate_executable_ddl(sql).is_ok());
    }

    #[test]
    fn valid_alter_add_index_passes() {
        let sql = "ALTER TABLE `t` ADD INDEX `idx_name` (`name`);";
        assert!(validate_executable_ddl(sql).is_ok());
    }

    #[test]
    fn valid_create_index_passes() {
        let sql = "CREATE INDEX idx ON t (a); CREATE UNIQUE INDEX uidx ON t (b);";
        let v = validate_executable_ddl(sql).unwrap();
        assert_eq!(v.statements.len(), 2);
        assert!(!v.has_high_risk());
    }

    #[test]
    fn alter_with_on_update_column_passes() {
        let sql =
            "ALTER TABLE t ADD COLUMN updated_at timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP;";
        assert!(validate_executable_ddl(sql).is_ok());
    }

    #[test]
    fn delete_update_truncate_are_high_risk() {
        for sql in [
            "DELETE FROM users WHERE id = 1;",
            "UPDATE t SET a = 1;",
            "TRUNCATE TABLE t;",
        ] {
            let v = validate_executable_ddl(sql).unwrap();
            assert!(v.has_high_risk(), "{sql}");
            assert_eq!(v.risks[0], StmtRisk::High);
        }
    }

    #[test]
    fn insert_on_duplicate_key_is_high_risk() {
        let sql = r#"
INSERT INTO sys_config (`Id`, `scene`, `key`, `value`, `explain`)
VALUES
  ('DemoFeatureSwitch', '演示功能', 'DemoFeatureSwitch', '0', '是否启用'),
  ('DemoFeatureRate', '演示功能', 'DemoFeatureRate', '0.13', '比率')
ON DUPLICATE KEY UPDATE
  `value` = VALUES(`value`),
  `explain` = VALUES(`explain`),
  `scene` = VALUES(`scene`);
"#;
        let v = validate_executable_ddl(sql).expect("upsert 应允许");
        assert_eq!(v.statements.len(), 1);
        assert_eq!(v.risks[0], StmtRisk::High);
    }

    #[test]
    fn replace_into_is_high_risk() {
        let v = validate_executable_ddl("REPLACE INTO t (id) VALUES (1);").unwrap();
        assert_eq!(v.risks[0], StmtRisk::High);
    }

    #[test]
    fn drop_table_and_drop_column_are_high_risk() {
        let v = validate_executable_ddl("DROP TABLE users;").unwrap();
        assert_eq!(v.risks[0], StmtRisk::High);

        let v2 = validate_executable_ddl("ALTER TABLE t DROP COLUMN c;").unwrap();
        assert_eq!(v2.risks[0], StmtRisk::High);
    }

    #[test]
    fn drop_database_forbidden_by_default() {
        assert!(validate_executable_ddl("DROP DATABASE foo;").is_err());
    }

    #[test]
    fn create_table_is_normal_by_default() {
        let sql = "-- 宝宝档案\nCREATE TABLE IF NOT EXISTS `baby_info` (id int);";
        let v = validate_executable_ddl(sql).expect("建表应常规放行");
        assert_eq!(v.statements.len(), 1);
        assert_eq!(v.risks[0], StmtRisk::Normal);
    }

    #[test]
    fn policy_can_forbid_create_table() {
        let mut policy = DdlPolicy::default();
        policy.create_table = DdlPolicyLevel::Forbidden;
        let err = validate_executable_ddl_with_policy("CREATE TABLE t (id int);", &policy)
            .unwrap_err();
        assert!(err.contains("不允许") || err.contains("设置"), "{err}");
    }

    #[test]
    fn policy_can_allow_drop_database_as_high() {
        let mut policy = DdlPolicy::default();
        policy.drop_database = DdlPolicyLevel::High;
        let v = validate_executable_ddl_with_policy("DROP DATABASE foo;", &policy).unwrap();
        assert_eq!(v.risks[0], StmtRisk::High);
    }

    #[test]
    fn empty_sql_rejected() {
        let err = validate_executable_ddl("   ").unwrap_err();
        assert!(err.contains("SQL 为空"));
    }

    #[test]
    fn comments_only_rejected() {
        let err = validate_executable_ddl("-- just a comment\n/* block */\n// hi").unwrap_err();
        assert!(err.contains("SQL 为空") || err.contains("注释"));
    }

    #[test]
    fn insert_inside_comments_does_not_block() {
        let sql = r#"
-- 历史曾用 INSERT INTO archive ...
/* 勿执行 INSERT */
// INSERT demo
ALTER TABLE t ADD COLUMN c int COMMENT '可 INSERT 字样';
"#;
        let v = validate_executable_ddl(sql).expect("注释中的 INSERT 不应拦截");
        assert_eq!(v.statements.len(), 1);
        assert!(v.statements[0].to_ascii_uppercase().contains("ADD COLUMN"));
        assert!(!v.statements[0].contains("历史曾用"));
    }

    #[test]
    fn strip_preserves_string_with_comment_markers() {
        let sql = "ALTER TABLE t ADD COLUMN c varchar(20) COMMENT 'a--b /*c*/ //d';";
        let v = validate_executable_ddl(sql).unwrap();
        assert!(v.statements[0].contains("a--b /*c*/ //d"));
    }

    #[test]
    fn mixed_normal_and_high() {
        let sql = "ALTER TABLE t ADD COLUMN c int; DELETE FROM t;";
        let v = validate_executable_ddl(sql).unwrap();
        assert_eq!(v.risks, vec![StmtRisk::Normal, StmtRisk::High]);
        assert_eq!(v.high_risk_count(), 1);
    }

    #[test]
    fn alter_change_column_with_drop_add_index_passes() {
        let sql = r#"
ALTER TABLE `white_noise`
    DROP INDEX `idx_white_noise_age_group`,
    CHANGE COLUMN `age_group_id` `category_id` varchar(64) NOT NULL,
    ADD INDEX `idx_white_noise_category` (`category_id`);
"#;
        let v = validate_executable_ddl(sql).expect("CHANGE COLUMN 应支持");
        assert_eq!(v.statements.len(), 1);
        // 含 DROP INDEX → 默认高风险
        assert_eq!(v.risks[0], StmtRisk::High);
    }

    #[test]
    fn alter_change_and_rename_column_are_safe_kinds() {
        let v = validate_executable_ddl(
            "ALTER TABLE t CHANGE COLUMN `a` `b` int NOT NULL;",
        )
        .unwrap();
        assert_eq!(v.risks[0], StmtRisk::Normal);

        let v2 = validate_executable_ddl("ALTER TABLE t RENAME COLUMN a TO b;").unwrap();
        assert_eq!(v2.risks[0], StmtRisk::Normal);

        let v3 = validate_executable_ddl("ALTER TABLE t MODIFY `name` varchar(128) NULL;").unwrap();
        assert_eq!(v3.risks[0], StmtRisk::Normal);
    }

    #[test]
    fn add_key_and_unique_forms_pass() {
        assert!(validate_executable_ddl("ALTER TABLE t ADD KEY `k` (`a`);").is_ok());
        assert!(validate_executable_ddl("ALTER TABLE t ADD UNIQUE INDEX `u` (`a`);").is_ok());
        assert!(validate_executable_ddl("ALTER TABLE t ADD UNIQUE KEY `u` (`a`);").is_ok());
    }

    #[test]
    fn alter_convert_charset_passes_as_normal() {
        let sql = "ALTER TABLE `white_noise` CONVERT TO CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;";
        let v = validate_executable_ddl(sql).expect("CONVERT TO 应放行");
        assert_eq!(v.risks[0], StmtRisk::Normal);
    }

    #[test]
    fn alter_engine_auto_increment_and_comment_pass() {
        let sql = "ALTER TABLE t ENGINE=InnoDB, AUTO_INCREMENT=100, COMMENT='demo';";
        let v = validate_executable_ddl(sql).unwrap();
        assert_eq!(v.risks[0], StmtRisk::Normal);
    }

    #[test]
    fn alter_ignore_table_supported() {
        let v = validate_executable_ddl("ALTER IGNORE TABLE t ADD COLUMN c int;").unwrap();
        assert_eq!(v.risks[0], StmtRisk::Normal);
    }

    #[test]
    fn add_constraint_foreign_key_allowed_as_safe() {
        let sql = "ALTER TABLE t ADD CONSTRAINT fk_x FOREIGN KEY (a) REFERENCES other(id);";
        let v = validate_executable_ddl(sql).unwrap();
        assert_eq!(v.risks[0], StmtRisk::Normal);
    }

    #[test]
    fn add_primary_key_and_check_allowed() {
        assert!(validate_executable_ddl("ALTER TABLE t ADD PRIMARY KEY (id);").is_ok());
        assert!(validate_executable_ddl("ALTER TABLE t ADD CONSTRAINT c_idx CHECK (x > 0);").is_ok());
    }
}
