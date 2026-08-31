import { describe, expect, it } from "vitest";
import type { PlanNode } from "../../lib/commands";
import { mapPlanNodeToSql } from "./sqlMapping";

const node = (operator: string, source?: string): PlanNode => ({
  id: "n1",
  operator,
  nativeName: operator.toUpperCase(),
  source: source ?? null,
  estimatedRows: null,
  actualRows: null,
  timingMs: null,
  rowsScanned: null,
  details: {},
});

describe("mapPlanNodeToSql", () => {
  const joinSql =
    "SELECT c.customer_name\nFROM orders o\nJOIN customers c ON o.customer_id = c.id\nWHERE o.amount > 20\nGROUP BY c.customer_name\nORDER BY total DESC\nLIMIT 5";

  it("maps unique sources, join, filter, aggregate, sort, and limit", () => {
    const scan = mapPlanNodeToSql(node("scan", "fixture.main.customers"), joinSql);
    expect(scan).toEqual({
      from: joinSql.indexOf("JOIN customers") + 5,
      to: joinSql.indexOf("JOIN customers") + "customers".length + 5,
    });
    expect(joinSql.slice(scan!.from, scan!.to)).toBe("customers");

    const range = (operator: string) => {
      const found = mapPlanNodeToSql(node(operator), joinSql)!;
      return joinSql.slice(found.from, found.to);
    };
    expect(range("join")).toBe("JOIN");
    expect(range("filter")).toBe("WHERE");
    expect(range("aggregate")).toBe("GROUP BY");
    expect(range("sort")).toBe("ORDER BY");
    expect(range("limit")).toBe("LIMIT");
  });

  it("refuses to highlight when the construct appears more than once", () => {
    const duplicated = "SELECT 1 WHERE x > 0 UNION ALL SELECT 2 WHERE y > 0";
    expect(mapPlanNodeToSql(node("filter"), duplicated)).toBeNull();
  });

  it("maps quoted schema-qualified sources and window functions", () => {
    const sql = 'SELECT row_number() OVER () FROM "main"."order items"';
    const scan = mapPlanNodeToSql(node("scan", 'fixture."main"."order items"'), sql);
    expect(sql.slice(scan!.from, scan!.to)).toBe('"order items"');
    const window = mapPlanNodeToSql(node("window"), sql);
    expect(sql.slice(window!.from, window!.to)).toBe("OVER");
  });

  it("ignores SQL inside strings and comments when mapping", () => {
    const sql = "SELECT '-- WHERE' AS note /* WHERE */ FROM orders WHERE x > 0";
    const filter = mapPlanNodeToSql(node("filter"), sql);
    expect(sql.slice(filter!.from, filter!.to)).toBe("WHERE");
    expect(filter!.from).toBe(sql.lastIndexOf("WHERE"));
  });

  it("returns null for operators without a reliable construct", () => {
    expect(mapPlanNodeToSql(node("projection"), "SELECT 1")).toBeNull();
    expect(mapPlanNodeToSql(node("union"), "SELECT 1")).toBeNull();
  });
});
