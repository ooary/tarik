import { describe, expect, it } from "vitest";
import type { PlanNode } from "../../lib/commands";
import { explainOperator, importantDetails } from "./explanations";

const node = (operator: string, nativeName = operator.toUpperCase()): PlanNode => ({
  id: "n1",
  operator,
  nativeName,
  source: null,
  estimatedRows: null,
  actualRows: null,
  timingMs: null,
  rowsScanned: null,
  details: {},
});

describe("flow explanations", () => {
  it("explains every common normalized operator in plain language", () => {
    for (const operator of [
      "scan",
      "filter",
      "projection",
      "join",
      "aggregate",
      "group",
      "count",
      "sum",
      "average",
      "minimum",
      "maximum",
      "summaries",
      "distinct",
      "sort",
      "limit",
      "union",
      "window",
      "result",
    ]) {
      const explanation = explainOperator(node(operator));
      expect(explanation.known).toBe(true);
      expect(explanation.summary.length).toBeGreaterThan(20);
      expect(explanation.inputLabel).not.toBe("");
      expect(explanation.outputLabel).not.toBe("");
    }
  });

  it("uses verified semantic copy before generic operator copy", () => {
    const explanation = explainOperator({
      ...node("count", "HASH_GROUP_BY"),
      semantic: {
        title: "Count non-null market values per group",
        summary: "COUNT(market) counts only non-null values for each group.",
        inputLabel: "Rows in each group",
        outputLabel: "One count per group",
        sqlRange: { from: 27, to: 40 },
        conceptOnly: false,
      },
    });
    expect(explanation.title).toBe("Count non-null market values per group");
    expect(explanation.summary).toContain("COUNT(market)");
  });

  it("keeps unknown operators truthful instead of inventing semantics", () => {
    const explanation = explainOperator(node("unknown", "FUTURE_OPERATOR"));
    expect(explanation.known).toBe(false);
    expect(explanation.title).toBe("FUTURE_OPERATOR");
    expect(explanation.summary).toMatch(/does not have a verified/);
  });

  it("orders important join/filter/group details before generic native keys", () => {
    const details = importantDetails({
      ...node("join", "HASH_JOIN"),
      details: {
        arbitrary: 1,
        Filters: "amount > 20",
        Conditions: "customer_id = id",
        "Join Type": "INNER",
      },
    });
    expect(details.map((detail) => detail.label)).toEqual([
      "Join Type",
      "Conditions",
      "Filters",
      "arbitrary",
    ]);
  });
});
