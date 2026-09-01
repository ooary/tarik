import { describe, expect, it } from "vitest";
import {
  isClearlyReadOnlySql,
  previewTableSql,
  qualifiedSqlName,
  quoteSqlIdentifier,
} from "./sqlText";

describe("SQL source affordances", () => {
  it("quotes unusual identifiers safely", () => {
    expect(quoteSqlIdentifier('order"lines')).toBe('"order""lines"');
    expect(qualifiedSqlName("analytics", "order lines")).toBe('"analytics"."order lines"');
  });

  it("builds a bounded table preview query", () => {
    expect(previewTableSql("main", "orders")).toBe('SELECT *\nFROM "main"."orders"\nLIMIT 100;');
  });

  it("classifies only clearly read-only statements for Actual Flow", () => {
    expect(isClearlyReadOnlySql("-- note\n/* nested /* x */ ok */ SELECT 1")).toBe(true);
    expect(isClearlyReadOnlySql("VALUES (1), (2)")).toBe(true);
    expect(isClearlyReadOnlySql("SHOW TABLES")).toBe(true);
    expect(isClearlyReadOnlySql("DESCRIBE orders")).toBe(true);
    expect(isClearlyReadOnlySql("INSERT INTO orders VALUES (1)")).toBe(false);
    expect(isClearlyReadOnlySql("CREATE TABLE t AS SELECT 1")).toBe(false);
    expect(isClearlyReadOnlySql("SELECT 1; DROP TABLE orders")).toBe(false);
    // WITH can contain mutating statements; conservative warning is safer.
    expect(isClearlyReadOnlySql("WITH x AS (SELECT 1) SELECT * FROM x")).toBe(false);
  });
});
