/// Split a SQL snapshot into top-level statements with source byte ranges.
///
/// Tracks single-quoted strings, double-quoted identifiers, and comments so a
/// semicolon inside a literal or comment does not split. Dollar-quoted strings
/// are rare in this product and are not special-cased.
pub fn split_statement_ranges(sql: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut has_content = false;
    let mut chars = sql.char_indices().peekable();
    let mut statement_start = 0usize;

    while let Some((position, c)) = chars.next() {
        match c {
            '\'' | '"' => {
                let quote = c;
                has_content = true;
                current.push(c);
                while let Some((_, next)) = chars.next() {
                    current.push(next);
                    if next == quote {
                        // A doubled quote inside a literal is an escape.
                        if quote == '\'' && chars.peek().is_some_and(|(_, next)| *next == '\'') {
                            current.push(chars.next().map(|(_, value)| value).unwrap_or('\''));
                        } else {
                            break;
                        }
                    }
                }
            }
            '-' if chars.peek().is_some_and(|(_, next)| *next == '-') => {
                current.push(c);
                for (_, next) in chars.by_ref() {
                    current.push(next);
                    if next == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek().is_some_and(|(_, next)| *next == '*') => {
                current.push(c);
                current.push(chars.next().map(|(_, value)| value).unwrap_or('*'));
                let mut depth = 1usize;
                while depth > 0 {
                    match chars.next() {
                        Some((_, '*')) if chars.peek().is_some_and(|(_, next)| *next == '/') => {
                            current.push('*');
                            current.push(chars.next().map(|(_, value)| value).unwrap_or('/'));
                            depth -= 1;
                        }
                        Some((_, '/')) if chars.peek().is_some_and(|(_, next)| *next == '*') => {
                            current.push('/');
                            current.push(chars.next().map(|(_, value)| value).unwrap_or('*'));
                            depth += 1;
                        }
                        Some((_, next)) => current.push(next),
                        None => break,
                    }
                }
            }
            ';' => {
                if has_content {
                    push_trimmed_statement(&mut statements, sql, statement_start, position);
                }
                current.clear();
                has_content = false;
                statement_start = position + c.len_utf8();
            }
            _ => {
                if !c.is_whitespace() {
                    has_content = true;
                }
                current.push(c);
            }
        }
    }

    if has_content {
        push_trimmed_statement(&mut statements, sql, statement_start, sql.len());
    }
    statements
}

pub fn split_statements(sql: &str) -> Vec<String> {
    split_statement_ranges(sql)
        .into_iter()
        .map(|(_, statement)| statement)
        .collect()
}

/// Conservative read-only boundary for custom quality SQL. Strings, quoted
/// identifiers, and comments are skipped before token classification.
pub fn validate_quality_read_only(sql: &str) -> Result<String, &'static str> {
    let statements = split_statements(sql);
    if statements.len() != 1 {
        return Err("custom quality SQL must contain exactly one statement");
    }
    let statement = statements.into_iter().next().unwrap_or_default();
    let tokens = lexical_tokens(&statement);
    if !matches!(tokens.first().map(String::as_str), Some("select" | "with")) {
        return Err("custom quality SQL must start with SELECT or WITH");
    }
    const FORBIDDEN: &[&str] = &[
        "insert",
        "update",
        "delete",
        "merge",
        "create",
        "alter",
        "drop",
        "truncate",
        "copy",
        "attach",
        "detach",
        "install",
        "load",
        "pragma",
        "call",
        "set",
        "reset",
        "vacuum",
        "checkpoint",
        "export",
        "import",
        "read_csv",
        "read_csv_auto",
        "read_parquet",
        "parquet_scan",
        "sqlite_scan",
        "postgres_scan",
        "httpfs",
    ];
    if tokens
        .iter()
        .any(|token| FORBIDDEN.contains(&token.as_str()))
    {
        return Err("custom quality SQL contains a mutating or external operation");
    }
    Ok(statement)
}

fn lexical_tokens(sql: &str) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == quote && bytes.get(index + 1) == Some(&quote) {
                        index += 2;
                    } else if bytes[index] == quote {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                let mut depth = 1usize;
                while index < bytes.len() && depth > 0 {
                    if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                        depth += 1;
                        index += 2;
                    } else if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                        depth -= 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                tokens.push(sql[start..index].to_ascii_lowercase());
            }
            _ => index += 1,
        }
    }
    tokens
}

fn push_trimmed_statement(
    statements: &mut Vec<(std::ops::Range<usize>, String)>,
    sql: &str,
    start: usize,
    end: usize,
) {
    let raw = &sql[start..end];
    let leading = raw.len() - raw.trim_start().len();
    let trailing = raw.trim_end().len();
    let from = start + leading;
    let to = start + trailing;
    statements.push((from..to, sql[from..to].to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_top_level_semicolons_only() {
        let sql = "SELECT ';' AS x;\n-- a; comment\nSELECT 1; /* ; */ SELECT 'a''b'";
        assert_eq!(
            split_statements(sql),
            vec![
                "SELECT ';' AS x",
                "-- a; comment\nSELECT 1",
                "/* ; */ SELECT 'a''b'",
            ]
        );
    }

    #[test]
    fn quoted_identifiers_do_not_split() {
        assert_eq!(
            split_statements("SELECT \"col;name\" FROM t;"),
            vec!["SELECT \"col;name\" FROM t"]
        );
    }

    #[test]
    fn statement_ranges_preserve_offsets_after_whitespace_and_semicolons() {
        let sql = "  SELECT 1;\n  SELECT missing FROM t  ;";
        let statements = split_statement_ranges(sql);
        assert_eq!(statements[0], (2..10, "SELECT 1".into()));
        let second = sql.find("SELECT missing").unwrap();
        assert_eq!(statements[1].0, second..sql.rfind("t").unwrap() + 1);
        assert_eq!(&sql[statements[1].0.clone()], statements[1].1);
    }

    #[test]
    fn empty_input_yields_no_statements() {
        assert!(split_statements("  ;  ; -- only comment\n").is_empty());
    }

    #[test]
    fn quality_sql_accepts_one_select_and_rejects_hidden_mutation_or_external_reads() {
        assert!(validate_quality_read_only(
            "WITH failures AS (SELECT 1 WHERE false) SELECT * FROM failures"
        )
        .is_ok());
        assert!(validate_quality_read_only("SELECT 'delete attach' AS words").is_ok());
        for sql in [
            "SELECT 1; SELECT 2",
            "DELETE FROM t",
            "WITH changed AS (DELETE FROM t RETURNING *) SELECT * FROM changed",
            "SELECT * FROM read_parquet('outside.parquet')",
            "SELECT 1 /* nested /* COPY t TO 'x' */ still comment */",
        ] {
            let result = validate_quality_read_only(sql);
            if sql.contains("nested") {
                assert!(result.is_ok(), "comments do not become operators");
            } else {
                assert!(result.is_err(), "unexpectedly accepted {sql}");
            }
        }
    }
}
