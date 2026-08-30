export function quoteSqlIdentifier(identifier: string): string {
  return `"${identifier.replace(/"/g, '""')}"`;
}

export function qualifiedSqlName(schema: string, object: string): string {
  return `${quoteSqlIdentifier(schema)}.${quoteSqlIdentifier(object)}`;
}

export function previewTableSql(schema: string, object: string, limit = 100): string {
  return `SELECT *\nFROM ${qualifiedSqlName(schema, object)}\nLIMIT ${limit};`;
}
