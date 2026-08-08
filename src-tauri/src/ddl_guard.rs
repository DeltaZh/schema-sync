//! 模式 2：结构 DDL 白名单校验（仅允许 ALTER TABLE / CREATE INDEX 类语句）

const FORBIDDEN_KEYWORDS: &[&str] = &[
    "DROP", "DELETE", "UPDATE", "TRUNCATE", "INSERT", "REPLACE",
];

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

/// 校验粘贴 SQL 是否均为允许的结构 DDL；通过则返回拆分后的语句列表
pub fn validate_structure_ddl(sql: &str) -> Result<Vec<String>, String> {
    let statements = split_statements(sql);
    if statements.is_empty() {
        return Err("SQL 为空".into());
    }
    for (idx, stmt) in statements.iter().enumerate() {
        validate_one_statement(stmt)
            .map_err(|e| format!("第 {} 条语句: {e}", idx + 1))?;
    }
    Ok(statements)
}

fn validate_one_statement(stmt: &str) -> Result<(), String> {
    let trimmed = stmt.trim();
    if trimmed.is_empty() {
        return Err("语句为空".into());
    }
    if let Some(kw) = find_forbidden_keyword(trimmed) {
        return Err(format!("不允许包含危险关键字: {kw}"));
    }
    if is_allowed_alter_table(trimmed) || is_allowed_create_index(trimmed) {
        Ok(())
    } else {
        Err("仅允许 ALTER TABLE（ADD/MODIFY 列或索引）及 CREATE [UNIQUE] INDEX".into())
    }
}

/// 去掉字符串字面量后再扫描危险词；`ON UPDATE` 视为列定义合法片段
fn find_forbidden_keyword(stmt: &str) -> Option<&'static str> {
    let mut normalized = strip_quoted_literals(stmt).to_ascii_uppercase();
    normalized = normalized.replace("ON UPDATE", "ON_UPDA_TE");
    for kw in FORBIDDEN_KEYWORDS {
        if contains_word(&normalized, kw) {
            return Some(kw);
        }
    }
    None
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

fn contains_word(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    let wbytes = word.as_bytes();
    if wbytes.len() > bytes.len() {
        return false;
    }
    for i in 0..=bytes.len().saturating_sub(wbytes.len()) {
        if bytes[i..i + wbytes.len()].eq_ignore_ascii_case(wbytes)
            && is_word_boundary(bytes, i, wbytes.len())
        {
            return true;
        }
    }
    false
}

fn is_word_boundary(bytes: &[u8], start: usize, len: usize) -> bool {
    let before = start == 0 || !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_';
    let end = start + len;
    let after = end >= bytes.len()
        || (!bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_');
    before && after
}

fn is_allowed_alter_table(stmt: &str) -> bool {
    let upper = stmt.to_ascii_uppercase();
    let rest = match strip_leading_keyword(&upper, "ALTER TABLE") {
        Some(r) => r.trim(),
        None => return false,
    };
    if rest.is_empty() {
        return false;
    }
    let (table, clauses) = match split_table_and_clauses(rest) {
        Some(v) => v,
        None => return false,
    };
    if table.is_empty() || clauses.is_empty() {
        return false;
    }
    for clause in split_alter_clauses(clauses) {
        if !is_allowed_alter_clause(clause.trim()) {
            return false;
        }
    }
    true
}

fn is_allowed_create_index(stmt: &str) -> bool {
    let upper = stmt.to_ascii_uppercase();
    upper.starts_with("CREATE UNIQUE INDEX ")
        || upper.starts_with("CREATE INDEX ")
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

/// `table_ref` 与 `clauses`（ALTER 后续部分）
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
    parts.into_iter().map(str::trim).filter(|s| !s.is_empty()).collect()
}

fn is_allowed_alter_clause(clause: &str) -> bool {
    let upper = clause.trim().to_ascii_uppercase();
    // 仅允许明确的列/索引形态；禁止 ADD CONSTRAINT … FOREIGN KEY 等靠子串误匹配
    upper.starts_with("ADD COLUMN ")
        || upper.starts_with("MODIFY COLUMN ")
        || upper.starts_with("ADD INDEX ")
        || upper.starts_with("ADD KEY ")
        || upper.starts_with("ADD UNIQUE INDEX ")
        || upper.starts_with("ADD UNIQUE KEY ")
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
        let stmts = validate_structure_ddl(sql).expect("should pass");
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn valid_alter_modify_column_passes() {
        let sql = "ALTER TABLE users MODIFY COLUMN name varchar(128) NOT NULL;";
        assert!(validate_structure_ddl(sql).is_ok());
    }

    #[test]
    fn valid_alter_add_index_passes() {
        let sql = "ALTER TABLE `t` ADD INDEX `idx_name` (`name`);";
        assert!(validate_structure_ddl(sql).is_ok());
    }

    #[test]
    fn valid_create_index_passes() {
        let sql = "CREATE INDEX idx ON t (a); CREATE UNIQUE INDEX uidx ON t (b);";
        assert!(validate_structure_ddl(sql).is_ok());
    }

    #[test]
    fn alter_with_on_update_column_passes() {
        let sql =
            "ALTER TABLE t ADD COLUMN updated_at timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP;";
        assert!(validate_structure_ddl(sql).is_ok());
    }

    #[test]
    fn delete_statement_rejected() {
        let sql = "DELETE FROM users WHERE id = 1;";
        let err = validate_structure_ddl(sql).unwrap_err();
        assert!(err.contains("DELETE") || err.contains("不允许"));
    }

    #[test]
    fn drop_statement_rejected() {
        let sql = "DROP TABLE users;";
        let err = validate_structure_ddl(sql).unwrap_err();
        assert!(err.contains("DROP") || err.contains("不允许"));
    }

    #[test]
    fn alter_drop_column_rejected() {
        let sql = "ALTER TABLE t DROP COLUMN c;";
        let err = validate_structure_ddl(sql).unwrap_err();
        assert!(err.contains("DROP"));
    }

    #[test]
    fn insert_and_truncate_rejected() {
        assert!(validate_structure_ddl("INSERT INTO t VALUES (1);").is_err());
        assert!(validate_structure_ddl("TRUNCATE TABLE t;").is_err());
        assert!(validate_structure_ddl("REPLACE INTO t VALUES (1);").is_err());
    }

    #[test]
    fn update_statement_rejected() {
        assert!(validate_structure_ddl("UPDATE t SET a = 1;").is_err());
    }

    #[test]
    fn create_table_rejected() {
        assert!(validate_structure_ddl("CREATE TABLE t (id int);").is_err());
    }

    #[test]
    fn empty_sql_rejected() {
        assert_eq!(validate_structure_ddl("   ").unwrap_err(), "SQL 为空");
    }

    #[test]
    fn mixed_valid_and_invalid_fails() {
        let sql = "ALTER TABLE t ADD COLUMN c int; DELETE FROM t;";
        let err = validate_structure_ddl(sql).unwrap_err();
        assert!(err.contains("第 2 条"));
    }

    #[test]
    fn add_key_and_unique_forms_pass() {
        assert!(validate_structure_ddl("ALTER TABLE t ADD KEY `k` (`a`);").is_ok());
        assert!(validate_structure_ddl("ALTER TABLE t ADD UNIQUE INDEX `u` (`a`);").is_ok());
        assert!(validate_structure_ddl("ALTER TABLE t ADD UNIQUE KEY `u` (`a`);").is_ok());
    }

    #[test]
    fn add_constraint_foreign_key_rejected() {
        let sql = "ALTER TABLE t ADD CONSTRAINT fk_x FOREIGN KEY (a) REFERENCES other(id);";
        let err = validate_structure_ddl(sql).unwrap_err();
        assert!(
            err.contains("仅允许") || err.contains("不允许"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn add_clause_with_index_substring_only_rejected() {
        // 不得仅因子串含 INDEX/KEY 就放行
        let sql = "ALTER TABLE t ADD CONSTRAINT c_idx CHECK (x > 0);";
        assert!(validate_structure_ddl(sql).is_err());
        let sql2 = "ALTER TABLE t ADD PRIMARY KEY (id);";
        assert!(validate_structure_ddl(sql2).is_err());
    }
}
