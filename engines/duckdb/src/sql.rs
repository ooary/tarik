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
}
