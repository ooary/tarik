import type { SQLConfig, SQLNamespace } from "@codemirror/lang-sql";

export type SqlTable = {
  schema: string;
  label: string;
  type?: "table" | "view";
  columns: string[];
};

export function buildSqlCompletionSchema(tables: SqlTable[]): SQLConfig["schema"] {
  if (tables.length === 0) return undefined;
  const schemas: Record<string, Record<string, readonly string[]>> = {};
  for (const table of tables) {
    schemas[table.schema] ??= {};
    schemas[table.schema][table.label] = table.columns;
  }
  return schemas as SQLNamespace;
}
