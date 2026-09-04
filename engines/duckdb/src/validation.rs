//! Non-executing DuckDB parse/bind validation.

use duckdb::Connection;
use tarik_engine_protocol::{DiagnosticSeverity, SqlDiagnostic, SqlValidation};

use crate::sql::split_statement_ranges;

const EXPLAIN_PREFIX: &str = "EXPLAIN (FORMAT JSON) ";
const MAX_DIAGNOSTICS: usize = 20;

pub fn validate(connection: &Connection, sql: &str, revision: u64) -> SqlValidation {
    let mut diagnostics = Vec::new();
    for (range, statement) in split_statement_ranges(sql) {
        if diagnostics.len() >= MAX_DIAGNOSTICS {
            break;
        }
        let explained = format!("{EXPLAIN_PREFIX}{statement}");
        match connection.prepare(&explained) {
            Err(error) => {
                let raw = error.to_string();
                let (from, to) = reliable_caret_range(sql, range.start, &statement, &raw)
                    .map(|range| (Some(range.0), Some(range.1)))
                    .unwrap_or((None, None));
                diagnostics.push(SqlDiagnostic {
                    code: diagnostic_code(&raw).into(),
                    message: concise_message(&raw),
                    severity: DiagnosticSeverity::Error,
                    from,
                    to,
                });
            }
            Ok(_) => {
                if let Some((keyword, offset)) = mutation_without_where(&statement) {
                    let byte_from = range.start + offset;
                    diagnostics.push(SqlDiagnostic {
                        code: "sql.mutation_without_where".into(),
                        message: format!(
                            "{keyword} has no top-level WHERE condition and may affect every row."
                        ),
                        severity: DiagnosticSeverity::Warning,
                        from: Some(sql[..byte_from].encode_utf16().count() as u32),
                        to: Some(sql[..byte_from + keyword.len()].encode_utf16().count() as u32),
                    });
                }
            }
        }
    }
    SqlValidation {
        revision,
        diagnostics,
    }
}

fn mutation_without_where(statement: &str) -> Option<(&'static str, usize)> {
    if !statement.is_ascii() {
        return None;
    }
    let words = top_level_words(statement);
    let (first, offset) = words.first()?;
    let keyword = match first.as_str() {
        "UPDATE" => "UPDATE",
        "DELETE" => "DELETE",
        _ => return None,
    };
    (!words.iter().any(|(word, _)| word == "WHERE")).then_some((keyword, *offset))
}

fn top_level_words(statement: &str) -> Vec<(String, usize)> {
    let bytes = statement.as_bytes();
    let mut words = Vec::new();
    let mut index = 0;
    let mut depth = 0usize;
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
                let mut nested = 1usize;
                while index < bytes.len() && nested > 0 {
                    if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                        nested += 1;
                        index += 2;
                    } else if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                        nested -= 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
            }
            b'(' => {
                depth += 1;
                index += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            byte if depth == 0 && (byte.is_ascii_alphabetic() || byte == b'_') => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                words.push((statement[start..index].to_ascii_uppercase(), start));
            }
            _ => index += 1,
        }
    }
    words
}

fn diagnostic_code(message: &str) -> &'static str {
    if message.starts_with("Parser Error:") {
        "sql.syntax"
    } else if message.starts_with("Binder Error:") {
        "sql.bind"
    } else if message.starts_with("Catalog Error:") {
        "sql.catalog"
    } else {
        "sql.validation"
    }
}

fn concise_message(message: &str) -> String {
    message
        .split("\n\nLINE ")
        .next()
        .unwrap_or(message)
        .trim()
        .to_string()
}

fn reliable_caret_range(
    full_sql: &str,
    statement_start: usize,
    statement: &str,
    message: &str,
) -> Option<(u32, u32)> {
    if !full_sql.is_ascii() || !statement.is_ascii() {
        return None;
    }
    let line_marker = message.rfind("\nLINE ")?;
    let excerpt_block = &message[line_marker + 1..];
    let mut lines = excerpt_block.lines();
    let header = lines.next()?;
    let excerpt = header.split_once(": ")?.1;
    let caret_line = lines.next()?;
    let caret_column = caret_line.find('^')?;
    if !caret_line[caret_column + 1..].trim().is_empty() {
        return None;
    }
    let line_number: usize = header
        .strip_prefix("LINE ")?
        .split_once(':')?
        .0
        .parse()
        .ok()?;
    let statement_line_start = nth_line_start(statement, line_number)?;
    let source_line = statement[statement_line_start..]
        .split('\n')
        .next()
        .unwrap_or_default();
    let prefix_chars = EXPLAIN_PREFIX.len();
    let excerpt_column = header.find(excerpt)?;
    let excerpt_caret = caret_column.checked_sub(excerpt_column)?;
    let source_column = if line_number == 1 {
        excerpt_caret.checked_sub(prefix_chars)?
    } else {
        excerpt_caret
    };
    if excerpt.strip_prefix(EXPLAIN_PREFIX).unwrap_or(excerpt) != source_line {
        return None;
    }
    if source_column >= source_line.len() {
        return None;
    }
    let byte_from = statement_start + statement_line_start + source_column;
    let byte_to = byte_from + token_length(&full_sql[byte_from..]);
    Some((
        full_sql[..byte_from].encode_utf16().count() as u32,
        full_sql[..byte_to].encode_utf16().count() as u32,
    ))
}

fn nth_line_start(statement: &str, line_number: usize) -> Option<usize> {
    if line_number == 0 {
        return None;
    }
    if line_number == 1 {
        return Some(0);
    }
    let mut seen = 1;
    for (index, byte) in statement.bytes().enumerate() {
        if byte == b'\n' {
            seen += 1;
            if seen == line_number {
                return Some(index + 1);
            }
        }
    }
    None
}

fn token_length(rest: &str) -> usize {
    let first = rest.as_bytes().first().copied();
    match first {
        Some(b'"') => {
            let mut index = 1;
            while index < rest.len() {
                if rest.as_bytes()[index] == b'"' {
                    if rest.as_bytes().get(index + 1) == Some(&b'"') {
                        index += 2;
                    } else {
                        return index + 1;
                    }
                } else {
                    index += 1;
                }
            }
            1
        }
        Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => rest
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
            .count()
            .max(1),
        Some(_) => 1,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_observed_duckdb_caret_to_original_statement() {
        let sql = "SELECT * FORM orders";
        let message = "Parser Error: syntax error at or near \"orders\"\n\nLINE 1: EXPLAIN (FORMAT JSON) SELECT * FORM orders\n                                            ^";
        let range = reliable_caret_range(sql, 0, sql, message).unwrap();
        assert_eq!(&sql[range.0 as usize..range.1 as usize], "orders");
    }

    #[test]
    fn maps_multiline_caret_with_statement_offset() {
        let sql = "SELECT 1;\n  SELECT missing\n  FROM range(2)";
        let statement_start = sql.find("SELECT missing").unwrap();
        let statement = &sql[statement_start..];
        let message = "Binder Error: Referenced column missing\n\nLINE 1: EXPLAIN (FORMAT JSON) SELECT missing\n                                     ^";
        let range = reliable_caret_range(sql, statement_start, statement, message).unwrap();
        assert_eq!(&sql[range.0 as usize..range.1 as usize], "missing");
    }

    #[test]
    fn warns_only_for_top_level_update_or_delete_without_where() {
        assert_eq!(
            mutation_without_where("UPDATE t SET x = 1"),
            Some(("UPDATE", 0))
        );
        assert_eq!(mutation_without_where("DELETE FROM t"), Some(("DELETE", 0)));
        assert_eq!(
            mutation_without_where("UPDATE t SET x = 1 WHERE id = 2"),
            None
        );
        assert_eq!(
            mutation_without_where("DELETE FROM t WHERE note = 'WHERE'"),
            None
        );
        assert_eq!(mutation_without_where("SELECT * FROM t"), None);
    }

    #[test]
    fn end_of_input_and_unmatched_excerpt_have_no_range() {
        assert_eq!(
            reliable_caret_range("SELECT (1", 0, "SELECT (1", "Parser Error: end"),
            None
        );
        let message = "Parser Error: bad\n\nLINE 1: EXPLAIN (FORMAT JSON) SELECT other\n                             ^";
        assert_eq!(
            reliable_caret_range("SELECT 1", 0, "SELECT 1", message),
            None
        );
    }
}
