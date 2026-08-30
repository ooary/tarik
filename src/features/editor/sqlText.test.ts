import { describe, expect, it } from "vitest";
import { previewTableSql, qualifiedSqlName, quoteSqlIdentifier } from "./sqlText";

describe("SQL source affordances", () => {
  it("quotes unusual identifiers safely", () => {
    expect(quoteSqlIdentifier('order"lines')).toBe('"order""lines"');
    expect(qualifiedSqlName("analytics", "order lines")).toBe(
      '"analytics"."order lines"',
    );
  });

  it("builds a bounded table preview query", () => {
    expect(previewTableSql("main", "orders")).toBe(
      'SELECT *\nFROM "main"."orders"\nLIMIT 100;',
    );
  });
});
