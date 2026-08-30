export function quoteSqlIdentifier(identifier: string): string {
  return `"${identifier.replace(/"/g, '""')}"`;
}

export function qualifiedSqlName(schema: string, object: string): string {
  return `${quoteSqlIdentifier(schema)}.${quoteSqlIdentifier(object)}`;
}

export function previewTableSql(schema: string, object: string, limit = 100): string {
  return `SELECT *\nFROM ${qualifiedSqlName(schema, object)}\nLIMIT ${limit};`;
}

/**
 * Count top-level statements for display. Quotes and comments are scanned so
 * semicolons inside literals do not split; matches the engine-side splitter's
 * observable behavior for typical editor content.
 */
export function countSqlStatements(sql: string): number {
  let count = 0;
  let hasContent = false;
  const chars = sql;
  let index = 0;
  while (index < chars.length) {
    const c = chars[index];
    if (c === "'" || c === '"') {
      const quote = c;
      hasContent = true;
      index += 1;
      while (index < chars.length) {
        if (chars[index] === quote) {
          if (quote === "'" && chars[index + 1] === "'") {
            index += 2;
            continue;
          }
          index += 1;
          break;
        }
        index += 1;
      }
      continue;
    }
    if (c === "-" && chars[index + 1] === "-") {
      while (index < chars.length && chars[index] !== "\n") index += 1;
      continue;
    }
    if (c === "/" && chars[index + 1] === "*") {
      let depth = 1;
      index += 2;
      while (index < chars.length && depth > 0) {
        if (chars[index] === "/" && chars[index + 1] === "*") {
          depth += 1;
          index += 2;
        } else if (chars[index] === "*" && chars[index + 1] === "/") {
          depth -= 1;
          index += 2;
        } else {
          index += 1;
        }
      }
      continue;
    }
    if (c === ";") {
      if (hasContent) count += 1;
      hasContent = false;
      index += 1;
      continue;
    }
    if (!/\s/.test(c)) hasContent = true;
    index += 1;
  }
  if (hasContent) count += 1;
  return count;
}
