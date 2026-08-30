/// Split a SQL snapshot into top-level statements.
///
/// Tracks single-quoted strings, double-quoted identifiers, and comments so a
/// semicolon inside a literal or comment does not split. Dollar-quoted strings
/// are rare in this product and are not special-cased.
pub fn split_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut has_content = false;
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {
                let quote = c;
                has_content = true;
                current.push(c);
                while let Some(next) = chars.next() {
                    current.push(next);
                    if next == quote {
                        // A doubled quote inside a literal is an escape.
                        if quote == '\'' && chars.peek() == Some(&'\'') {
                            current.push(chars.next().unwrap_or('\''));
                        } else {
                            break;
                        }
                    }
                }
            }
            '-' if chars.peek() == Some(&'-') => {
                current.push(c);
                for next in chars.by_ref() {
                    current.push(next);
                    if next == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                current.push(c);
                current.push(chars.next().unwrap_or('*'));
                let mut depth = 1usize;
                while depth > 0 {
                    match chars.next() {
                        Some('*') if chars.peek() == Some(&'/') => {
                            current.push('*');
                            current.push(chars.next().unwrap_or('/'));
                            depth -= 1;
                        }
                        Some('/') if chars.peek() == Some(&'*') => {
                            current.push('/');
                            current.push(chars.next().unwrap_or('*'));
                            depth += 1;
                        }
                        Some(next) => current.push(next),
                        None => break,
                    }
                }
            }
            ';' => {
                if has_content {
                    statements.push(current.trim().to_string());
                }
                current.clear();
                has_content = false;
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
        statements.push(current.trim().to_string());
    }
    statements
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
    fn empty_input_yields_no_statements() {
        assert!(split_statements("  ;  ; -- only comment\n").is_empty());
    }
}
