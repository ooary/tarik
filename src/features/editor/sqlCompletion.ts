import {
  type Completion,
  type CompletionContext,
  type CompletionResult,
  type CompletionSource,
} from "@codemirror/autocomplete";
import type { SQLConfig, SQLNamespace } from "@codemirror/lang-sql";

export type SqlTable = {
  schema: string;
  label: string;
  type?: "table" | "view";
  columns: string[];
};

type Token = {
  text: string;
  upper: string;
  from: number;
  to: number;
  depth: number;
  quoted: boolean;
};

type AliasBinding = { alias: string; table: SqlTable };

const RESERVED = new Set(
  "ALL ALTER AND AS ASC BY CASE CREATE DELETE DESC DISTINCT DROP ELSE END EXISTS FALSE FROM FULL GROUP HAVING IN INNER INSERT INTO IS JOIN LEFT LIMIT NOT NULL ON OR ORDER OUTER RIGHT SELECT SET TABLE THEN TRUE UNION UPDATE USING VALUES VIEW WHEN WHERE WITH".split(
    " ",
  ),
);

const catalogSection = { name: "Project catalog", rank: 0 } as const;
const columnSection = { name: "Columns", rank: 1 } as const;

export function buildSqlCompletionSchema(tables: SqlTable[]): SQLConfig["schema"] {
  if (tables.length === 0) return undefined;
  const schemas: Record<string, Record<string, readonly string[]>> = {};
  for (const table of tables) {
    schemas[table.schema] ??= {};
    schemas[table.schema][table.label] = table.columns;
  }
  return schemas as SQLNamespace;
}

export function quoteCompletionIdentifier(identifier: string): string {
  if (/^[A-Za-z_][A-Za-z0-9_$]*$/.test(identifier) && !RESERVED.has(identifier.toUpperCase())) {
    return identifier;
  }
  return `"${identifier.replace(/"/g, '""')}"`;
}

export function createCatalogCompletionSource(tables: SqlTable[]): CompletionSource {
  return (context) => catalogCompletions(context, tables);
}

export function catalogCompletions(
  context: Pick<CompletionContext, "state" | "pos" | "explicit" | "matchBefore">,
  tables: SqlTable[],
): CompletionResult | null {
  if (tables.length === 0) return null;
  const sql = context.state.doc.toString();
  const before = sql.slice(0, context.pos);
  const word = context.matchBefore(/[\w$]*$/);
  const afterDot = /\.\s*[\w$]*$/.test(before);
  if (!context.explicit && !afterDot && (!word || word.from === word.to)) return null;
  const from = word?.from ?? context.pos;
  const prefix = word?.text ?? "";
  const tokens = tokenize(before.slice(0, from));
  const previous = tokens[tokens.length - 1];

  if (previous?.text === ".") {
    const qualifier = tokens[tokens.length - 2];
    if (!qualifier || (!qualifier.quoted && !isWord(qualifier.text))) return null;
    const name = unquote(qualifier.text).toLowerCase();
    const aliases = resolveAliases(sql, tables);
    const alias = aliases.find((binding) => binding.alias.toLowerCase() === name);
    if (alias) {
      return result(from, prefix, alias.table.columns.map(columnCompletion));
    }
    const schemaTables = tables.filter((table) => table.schema.toLowerCase() === name);
    if (schemaTables.length > 0) {
      return result(
        from,
        prefix,
        schemaTables.map((table) => relationCompletion(table)),
      );
    }
    return null;
  }

  if (isRelationContext(tokens)) {
    const duplicateNames = duplicateRelationNames(tables);
    const relations = tables.map((table) =>
      relationCompletion(table, duplicateNames.has(table.label.toLowerCase())),
    );
    const schemas = [...new Set(tables.map((table) => table.schema))].map(schemaCompletion);
    return result(from, prefix, [...relations, ...schemas]);
  }

  if (!context.explicit && !prefix) return null;
  const referenced = referencedTables(sql, tables);
  const columns = unambiguousColumns(referenced).map(columnCompletion);
  return columns.length > 0 ? result(from, prefix, columns) : null;
}

function result(from: number, prefix: string, options: Completion[]): CompletionResult {
  return {
    from,
    options,
    validFor: /^[\w$]*$/,
    filter: true,
    ...(prefix ? {} : { to: from }),
  };
}

function relationCompletion(table: SqlTable, qualify = false): Completion {
  const apply = qualify
    ? `${quoteCompletionIdentifier(table.schema)}.${quoteCompletionIdentifier(table.label)}`
    : quoteCompletionIdentifier(table.label);
  return {
    label: table.label,
    apply,
    type: table.type === "view" ? "interface" : "class",
    detail: table.type === "view" ? `view · ${table.schema}` : `table · ${table.schema}`,
    boost: 90,
    section: catalogSection,
  };
}

function schemaCompletion(schema: string): Completion {
  return {
    label: schema,
    apply: quoteCompletionIdentifier(schema),
    type: "namespace",
    detail: "schema",
    boost: 70,
    section: catalogSection,
    commitCharacters: ["."],
  };
}

function columnCompletion(column: string): Completion {
  return {
    label: column,
    apply: quoteCompletionIdentifier(column),
    type: "property",
    detail: "column",
    boost: 80,
    section: columnSection,
  };
}

function isRelationContext(tokens: Token[]): boolean {
  const previous = tokens[tokens.length - 1];
  if (!previous) return false;
  if (previous.depth !== 0) return false;
  return previous.upper === "FROM" || previous.upper === "JOIN";
}

function resolveAliases(sql: string, tables: SqlTable[]): AliasBinding[] {
  const tokens = tokenize(sql);
  const bindings: AliasBinding[] = [];
  for (let index = 0; index < tokens.length; index += 1) {
    const introducer = tokens[index];
    if (introducer.depth !== 0 || !["FROM", "JOIN"].includes(introducer.upper)) continue;
    let cursor = index + 1;
    const first = tokens[cursor];
    if (!first || !isIdentifier(first)) continue;
    let schema: string | null = null;
    let relation = unquote(first.text);
    if (tokens[cursor + 1]?.text === "." && isIdentifier(tokens[cursor + 2])) {
      schema = relation;
      relation = unquote(tokens[cursor + 2].text);
      cursor += 2;
    }
    const candidates = tables.filter(
      (table) =>
        table.label.toLowerCase() === relation.toLowerCase() &&
        (!schema || table.schema.toLowerCase() === schema.toLowerCase()),
    );
    if (candidates.length !== 1) continue;
    cursor += 1;
    if (tokens[cursor]?.upper === "AS") cursor += 1;
    const aliasToken = tokens[cursor];
    if (
      aliasToken &&
      aliasToken.depth === 0 &&
      isIdentifier(aliasToken) &&
      !clauseKeyword(aliasToken.upper)
    ) {
      bindings.push({ alias: unquote(aliasToken.text), table: candidates[0] });
    }
  }
  return uniqueAliases(bindings);
}

function referencedTables(sql: string, tables: SqlTable[]): SqlTable[] {
  const tokens = tokenize(sql);
  const found: SqlTable[] = [];
  for (let index = 0; index < tokens.length; index += 1) {
    if (tokens[index].depth !== 0 || !["FROM", "JOIN"].includes(tokens[index].upper)) continue;
    const first = tokens[index + 1];
    if (!first || !isIdentifier(first)) continue;
    let schema: string | null = null;
    let relation = unquote(first.text);
    if (tokens[index + 2]?.text === "." && isIdentifier(tokens[index + 3])) {
      schema = relation;
      relation = unquote(tokens[index + 3].text);
    }
    const matches = tables.filter(
      (table) =>
        table.label.toLowerCase() === relation.toLowerCase() &&
        (!schema || table.schema.toLowerCase() === schema.toLowerCase()),
    );
    if (matches.length === 1 && !found.includes(matches[0])) found.push(matches[0]);
  }
  return found;
}

function unambiguousColumns(tables: SqlTable[]): string[] {
  const counts = new Map<string, { label: string; count: number }>();
  for (const table of tables) {
    for (const column of table.columns) {
      const key = column.toLowerCase();
      const current = counts.get(key);
      counts.set(key, { label: current?.label ?? column, count: (current?.count ?? 0) + 1 });
    }
  }
  return [...counts.values()]
    .filter((entry) => entry.count === 1)
    .map((entry) => entry.label)
    .sort((left, right) => left.localeCompare(right));
}

function duplicateRelationNames(tables: SqlTable[]): Set<string> {
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  for (const table of tables) {
    const key = table.label.toLowerCase();
    if (seen.has(key)) duplicates.add(key);
    seen.add(key);
  }
  return duplicates;
}

function uniqueAliases(bindings: AliasBinding[]): AliasBinding[] {
  const counts = new Map<string, number>();
  for (const binding of bindings) {
    const key = binding.alias.toLowerCase();
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return bindings.filter((binding) => counts.get(binding.alias.toLowerCase()) === 1);
}

function clauseKeyword(value: string): boolean {
  return [
    "ON",
    "JOIN",
    "LEFT",
    "RIGHT",
    "FULL",
    "INNER",
    "OUTER",
    "WHERE",
    "GROUP",
    "ORDER",
    "LIMIT",
    "UNION",
    "HAVING",
    "QUALIFY",
  ].includes(value);
}

function isIdentifier(token: Token | undefined): token is Token {
  return Boolean(token && (token.quoted || isWord(token.text)));
}

function isWord(value: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_$]*$/.test(value);
}

function unquote(value: string): string {
  return value.replace(/^"|"$/g, "").replace(/""/g, '"');
}

function tokenize(sql: string): Token[] {
  const tokens: Token[] = [];
  let index = 0;
  let depth = 0;
  while (index < sql.length) {
    const char = sql[index];
    if (/\s/.test(char)) {
      index += 1;
      continue;
    }
    if (char === "-" && sql[index + 1] === "-") {
      index += 2;
      while (index < sql.length && sql[index] !== "\n") index += 1;
      continue;
    }
    if (char === "/" && sql[index + 1] === "*") {
      let nested = 1;
      index += 2;
      while (index < sql.length && nested > 0) {
        if (sql[index] === "/" && sql[index + 1] === "*") {
          nested += 1;
          index += 2;
        } else if (sql[index] === "*" && sql[index + 1] === "/") {
          nested -= 1;
          index += 2;
        } else index += 1;
      }
      continue;
    }
    if (char === "'") {
      index += 1;
      while (index < sql.length) {
        if (sql[index] === "'" && sql[index + 1] === "'") index += 2;
        else if (sql[index] === "'") {
          index += 1;
          break;
        } else index += 1;
      }
      continue;
    }
    if (char === '"') {
      const from = index;
      index += 1;
      while (index < sql.length) {
        if (sql[index] === '"' && sql[index + 1] === '"') index += 2;
        else if (sql[index] === '"') {
          index += 1;
          break;
        } else index += 1;
      }
      const text = sql.slice(from, index);
      tokens.push({
        text,
        upper: unquote(text).toUpperCase(),
        from,
        to: index,
        depth,
        quoted: true,
      });
      continue;
    }
    if (char === "(") {
      tokens.push({ text: char, upper: char, from: index, to: index + 1, depth, quoted: false });
      depth += 1;
      index += 1;
      continue;
    }
    if (char === ")") {
      depth = Math.max(0, depth - 1);
      tokens.push({ text: char, upper: char, from: index, to: index + 1, depth, quoted: false });
      index += 1;
      continue;
    }
    if (/[A-Za-z_]/.test(char)) {
      const from = index;
      index += 1;
      while (index < sql.length && /[A-Za-z0-9_$]/.test(sql[index])) index += 1;
      const text = sql.slice(from, index);
      tokens.push({ text, upper: text.toUpperCase(), from, to: index, depth, quoted: false });
      continue;
    }
    tokens.push({ text: char, upper: char, from: index, to: index + 1, depth, quoted: false });
    index += 1;
  }
  return tokens;
}
