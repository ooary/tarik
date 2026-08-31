import type { PlanNode } from "../../lib/commands";

export interface SqlRange {
  from: number;
  to: number;
}

interface Token extends SqlRange {
  text: string;
  upper: string;
  kind: "word" | "quoted" | "symbol";
}

/**
 * Conservative plan-node -> SQL mapper. A range is returned only when the
 * relevant construct occurs exactly once in the immutable SQL snapshot.
 */
export function mapPlanNodeToSql(node: PlanNode, sql: string): SqlRange | null {
  const tokens = tokenize(sql);
  if (node.operator === "scan" && node.source) {
    const sourceName = lastQualifiedPart(node.source);
    const candidates = tokens.filter((token, index) => {
      if (token.kind !== "word" && token.kind !== "quoted") return false;
      if (unquote(token.text).toLowerCase() !== sourceName.toLowerCase()) return false;
      // The relation may be schema-qualified; walk backward over `schema .` to
      // find the FROM/JOIN introducer without claiming unrelated columns.
      let cursor = index - 1;
      while (cursor >= 1 && tokens[cursor]?.text === ".") cursor -= 2;
      return tokens[cursor]?.upper === "FROM" || tokens[cursor]?.upper === "JOIN";
    });
    return uniqueRange(candidates);
  }
  const phraseByOperator: Record<string, string[]> = {
    filter: ["WHERE"],
    join: ["JOIN"],
    aggregate: ["GROUP", "BY"],
    sort: ["ORDER", "BY"],
    limit: ["LIMIT"],
    union: ["UNION"],
    window: ["OVER"],
  };
  const phrase = phraseByOperator[node.operator];
  if (!phrase) return null;
  return uniqueRange(findPhrase(tokens, phrase));
}

function findPhrase(tokens: Token[], phrase: string[]): SqlRange[] {
  const matches: SqlRange[] = [];
  for (let index = 0; index <= tokens.length - phrase.length; index += 1) {
    if (phrase.every((word, offset) => tokens[index + offset]?.upper === word)) {
      matches.push({ from: tokens[index].from, to: tokens[index + phrase.length - 1].to });
    }
  }
  return matches;
}

function uniqueRange(ranges: SqlRange[]): SqlRange | null {
  if (ranges.length !== 1) return null;
  return { from: ranges[0].from, to: ranges[0].to };
}

function lastQualifiedPart(source: string): string {
  const parts = source.split(".");
  return unquote(parts[parts.length - 1] ?? source);
}

function unquote(value: string): string {
  return value.replace(/^"|"$/g, "").replace(/""/g, '"');
}

/** SQL tokenizer that skips strings and comments while preserving ranges. */
function tokenize(sql: string): Token[] {
  const tokens: Token[] = [];
  let index = 0;
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
      let depth = 1;
      index += 2;
      while (index < sql.length && depth > 0) {
        if (sql[index] === "/" && sql[index + 1] === "*") {
          depth += 1;
          index += 2;
        } else if (sql[index] === "*" && sql[index + 1] === "/") {
          depth -= 1;
          index += 2;
        } else {
          index += 1;
        }
      }
      continue;
    }
    if (char === "'") {
      index += 1;
      while (index < sql.length) {
        if (sql[index] === "'" && sql[index + 1] === "'") {
          index += 2;
        } else if (sql[index] === "'") {
          index += 1;
          break;
        } else {
          index += 1;
        }
      }
      continue;
    }
    if (char === '"') {
      const from = index;
      index += 1;
      while (index < sql.length) {
        if (sql[index] === '"' && sql[index + 1] === '"') {
          index += 2;
        } else if (sql[index] === '"') {
          index += 1;
          break;
        } else {
          index += 1;
        }
      }
      const text = sql.slice(from, index);
      tokens.push({ from, to: index, text, upper: unquote(text).toUpperCase(), kind: "quoted" });
      continue;
    }
    if (/[A-Za-z_]/.test(char)) {
      const from = index;
      index += 1;
      while (index < sql.length && /[A-Za-z0-9_$]/.test(sql[index])) index += 1;
      const text = sql.slice(from, index);
      tokens.push({ from, to: index, text, upper: text.toUpperCase(), kind: "word" });
      continue;
    }
    tokens.push({ from: index, to: index + 1, text: char, upper: char, kind: "symbol" });
    index += 1;
  }
  return tokens;
}
