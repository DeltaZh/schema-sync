//! DDL 投放语句校验：常规结构变更 + 高风险语句（执行时需二次确认）

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

/// 校验可投放 SQL；通过则返回语句列表与逐条风险。
/// 会先剥离注释再拆分；返回的语句为去注释后的可执行文本。
pub fn validate_executable_ddl(sql: &str) -> Result<ValidatedDdl, String> {
    let cleaned = strip_sql_comments(sql);
    let statements = split_statements(&cleaned);
    if statements.is_empty() {
        return Err("SQL 为空（或仅含注释）".into());
    }
    let mut risks = Vec::with_capacity(statements.len());
    for (idx, stmt) in statements.iter().enumerate() {
        let risk = classify_statement(stmt)
            .map_err(|e| format!("第 {} 条语句: {e}", idx + 1))?;
        risks.push(risk);
    }
    Ok(ValidatedDdl { statements, risks })
}

/// 兼容旧名：校验并仅返回语句列表
pub fn validate_structure_ddl(sql: &str) -> Result<Vec<String>, String> {
    Ok(validate_executable_ddl(sql)?.statements)
}

fn classify_statement(stmt: &str) -> Result<StmtRisk, String> {
    let trimmed = stmt.trim();
    if trimmed.is_empty() {
        return Err("语句为空".into());
    }

    let upper = strip_quoted_literals(trimmed).to_ascii_uppercase();
    // 避免列定义 ON UPDATE 干扰；INSERT 的 ON DUPLICATE KEY UPDATE 仍按 INSERT 识别
    let upper_for_head = upper.replace("ON UPDATE", "ON_UPDA_TE");
    let head = upper_for_head.trim_start();

    if starts_with_keyword(head, "DROP DATABASE") || starts_with_keyword(head, "DROP SCHEMA") {
        return Err("不允许 DROP DATABASE / DROP SCHEMA".into());
    }

    // 数据写入 / 变更类：一律高风险，执行需二次确认
    if starts_with_keyword(head, "INSERT")
        || starts_with_keyword(head, "REPLACE")
        || starts_with_keyword(head, "DELETE")
        || starts_with_keyword(head, "UPDATE")
        || starts_with_keyword(head, "TRUNCATE")
    {
        return Ok(StmtRisk::High);
    }
    if starts_with_keyword(head, "DROP TABLE") {
        return Ok(StmtRisk::High);
    }
    if starts_with_keyword(head, "DROP INDEX") {
        return Ok(StmtRisk::High);
    }
    if starts_with_keyword(head, "CREATE UNIQUE INDEX")
        || starts_with_keyword(head, "CREATE INDEX")
    {
        return Ok(StmtRisk::Normal);
    }
    if starts_with_keyword(head, "ALTER TABLE") {
        return classify_alter_table(trimmed);
    }

    Err(
        "仅允许 ALTER TABLE / CREATE INDEX，以及高风险的 INSERT/REPLACE/DROP/DELETE/UPDATE/TRUNCATE（执行需二次确认）"
            .into(),
    )
}

fn classify_alter_table(stmt: &str) -> Result<StmtRisk, String> {
    let upper = stmt.to_ascii_uppercase();
    let rest = match strip_leading_keyword(&upper, "ALTER TABLE") {
        Some(r) => r.trim(),
        None => return Err("无法解析 ALTER TABLE".into()),
    };
    if rest.is_empty() {
        return Err("ALTER TABLE 缺少表名或子句".into());
    }
    let (_table, clauses) = split_table_and_clauses(rest)
        .ok_or_else(|| "无法解析 ALTER TABLE 子句".to_string())?;
    if clauses.is_empty() {
        return Err("ALTER TABLE 缺少变更子句".into());
    }

    let mut risk = StmtRisk::Normal;
    for clause in split_alter_clauses(clauses) {
        match classify_alter_clause(clause.trim()) {
            Some(StmtRisk::High) => risk = StmtRisk::High,
            Some(StmtRisk::Normal) => {}
            None => {
                return Err(format!(
                    "不支持的 ALTER 子句: {}",
                    truncate_for_err(clause.trim())
                ));
            }
        }
    }
    Ok(risk)
}

fn classify_alter_clause(clause: &str) -> Option<StmtRisk> {
    let upper = clause.trim().to_ascii_uppercase();
    if upper.starts_with("ADD COLUMN ")
        || upper.starts_with("MODIFY COLUMN ")
        || upper.starts_with("ADD INDEX ")
        || upper.starts_with("ADD KEY ")
        || upper.starts_with("ADD UNIQUE INDEX ")
        || upper.starts_with("ADD UNIQUE KEY ")
    {
        return Some(StmtRisk::Normal);
    }
    if upper.starts_with("DROP COLUMN ")
        || upper.starts_with("DROP INDEX ")
        || upper.starts_with("DROP KEY ")
        || upper.starts_with("DROP PRIMARY KEY")
    {
        return Some(StmtRisk::High);
    }
    // COMMENT 修改
    if upper.starts_with("COMMENT ") || upper == "COMMENT" || upper.starts_with("COMMENT=") {
        return Some(StmtRisk::Normal);
    }
    None
}

fn truncate_for_err(s: &str) -> String {
    const MAX: usize = 48;
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= MAX {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
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
    fn drop_database_rejected() {
        assert!(validate_executable_ddl("DROP DATABASE foo;").is_err());
    }

    #[test]
    fn create_table_rejected() {
        assert!(validate_executable_ddl("CREATE TABLE t (id int);").is_err());
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
    fn add_key_and_unique_forms_pass() {
        assert!(validate_executable_ddl("ALTER TABLE t ADD KEY `k` (`a`);").is_ok());
        assert!(validate_executable_ddl("ALTER TABLE t ADD UNIQUE INDEX `u` (`a`);").is_ok());
        assert!(validate_executable_ddl("ALTER TABLE t ADD UNIQUE KEY `u` (`a`);").is_ok());
    }

    #[test]
    fn add_constraint_foreign_key_rejected() {
        let sql = "ALTER TABLE t ADD CONSTRAINT fk_x FOREIGN KEY (a) REFERENCES other(id);";
        let err = validate_executable_ddl(sql).unwrap_err();
        assert!(
            err.contains("不支持") || err.contains("仅允许") || err.contains("不允许"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn add_clause_with_index_substring_only_rejected() {
        let sql = "ALTER TABLE t ADD CONSTRAINT c_idx CHECK (x > 0);";
        assert!(validate_executable_ddl(sql).is_err());
        let sql2 = "ALTER TABLE t ADD PRIMARY KEY (id);";
        assert!(validate_executable_ddl(sql2).is_err());
    }
}
