import { CompletionContext } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import {
  buildSqlCompletionSchema,
  catalogCompletions,
  quoteCompletionIdentifier,
  type SqlTable,
} from "./sqlCompletion";

const tables: SqlTable[] = [
  {
    schema: "main",
    label: "data_2021",
    type: "table",
    columns: ["commodity", "market", "order value"],
  },
  {
    schema: "main",
    label: "market_summary",
    type: "view",
    columns: ["commodity", "market_count"],
  },
  {
    schema: "archive",
    label: "data_2021",
    type: "view",
    columns: ["commodity", "legacy_market"],
  },
];

function completions(sql: string, cursor = sql.length, explicit = false) {
  const state = EditorState.create({ doc: sql });
  const context = new CompletionContext(state, cursor, explicit);
  return catalogCompletions(context, tables);
}

describe("catalog SQL completion", () => {
  it("preserves the legacy schema hierarchy", () => {
    expect(buildSqlCompletionSchema(tables)).toEqual({
      main: {
        data_2021: ["commodity", "market", "order value"],
        market_summary: ["commodity", "market_count"],
      },
      archive: { data_2021: ["commodity", "legacy_market"] },
    });
  });

  it("suggests and distinguishes tables and views after FROM and JOIN", () => {
    for (const sql of ["SELECT * FROM data_", "SELECT * FROM orders o JOIN market_"]) {
      const result = completions(sql)!;
      expect(result.from).toBe(sql.lastIndexOf(sql.endsWith("data_") ? "data_" : "market_"));
      const data = result.options.find((option) => option.label === "data_2021")!;
      expect(data.detail).toBe("table · main");
      expect(data.apply).toBe("main.data_2021");
      const view = result.options.find((option) => option.label === "market_summary")!;
      expect(view.detail).toBe("view · main");
    }
  });

  it("scopes schema-qualified completion and quotes accepted names", () => {
    const result = completions("SELECT * FROM main.")!;
    expect(result.options.map((option) => option.label)).toEqual(["data_2021", "market_summary"]);
    expect(result.options[0].apply).toBe("data_2021");
    expect(quoteCompletionIdentifier("order value")).toBe('"order value"');
    expect(quoteCompletionIdentifier("select")).toBe('"select"');
    expect(quoteCompletionIdentifier('say"what')).toBe('"say""what"');
    expect(quoteCompletionIdentifier("ordinary_name")).toBe("ordinary_name");
  });

  it("resolves aliases even when FROM appears after the SELECT cursor", () => {
    const sql = 'SELECT d. FROM "main"."data_2021" AS d';
    const cursor = sql.indexOf("d.") + 2;
    const result = completions(sql, cursor)!;
    expect(result.options.map((option) => option.label)).toEqual([
      "commodity",
      "market",
      "order value",
    ]);
    expect(result.options[2].apply).toBe('"order value"');
  });

  it("does not claim columns for ambiguous aliases", () => {
    const sql = "SELECT d. FROM main.data_2021 d JOIN archive.data_2021 d ON true";
    const cursor = sql.indexOf("d.") + 2;
    expect(completions(sql, cursor)).toBeNull();
  });

  it("offers only unambiguous columns from referenced sources on manual completion", () => {
    const sql = "SELECT  FROM main.data_2021 d JOIN main.market_summary m ON true";
    const cursor = "SELECT ".length;
    const result = completions(sql, cursor, true)!;
    const labels = result.options.map((option) => option.label);
    expect(labels).toContain("market");
    expect(labels).toContain("market_count");
    expect(labels).toContain("order value");
    expect(labels).not.toContain("commodity");
  });

  it("returns no catalog result for an empty catalog or unrelated implicit context", () => {
    const state = EditorState.create({ doc: "SELECT" });
    const context = new CompletionContext(state, 6, false);
    expect(catalogCompletions(context, [])).toBeNull();
    expect(completions("SELECT ")).toBeNull();
  });
});
