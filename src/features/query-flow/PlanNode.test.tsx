import { ReactFlowProvider } from "@xyflow/react";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { PlanNode } from "../../lib/commands";
import { PlanNodeCard } from "./PlanNode";

function renderNode(planNode: PlanNode) {
  return render(
    <ReactFlowProvider>
      <PlanNodeCard {...props(planNode)} />
    </ReactFlowProvider>,
  );
}

function props(planNode: PlanNode): Parameters<typeof PlanNodeCard>[0] {
  return {
    id: planNode.id,
    data: { traversalDelayMs: 0, planNode },
    selected: false,
  } as unknown as Parameters<typeof PlanNodeCard>[0];
}

describe("PlanNodeCard row labels", () => {
  it("labels Explain cardinality as a DuckDB output estimate", () => {
    renderNode({
      id: "n0",
      operator: "filter",
      nativeName: "FILTER",
      source: null,
      estimatedRows: 147,
      actualRows: null,
      timingMs: null,
      rowsScanned: null,
      details: {},
    });
    expect(screen.getByText("DuckDB estimate · ~147 output rows")).toHaveAttribute(
      "title",
      expect.stringMatching(/planning guess.*not the actual query result/i),
    );
  });

  it("compares Profile estimate and actual output with a ratio badge", () => {
    renderNode({
      id: "n0",
      operator: "filter",
      nativeName: "FILTER",
      source: null,
      estimatedRows: 147,
      actualRows: 4,
      timingMs: 0.01,
      rowsScanned: 734,
      details: {},
    });
    expect(screen.getByText("Est. ~147 · Actual 4")).toHaveAttribute(
      "title",
      "Rows that actually left this operation during Profile",
    );
    expect(screen.getByText("Over-estimate · 36.8×")).toHaveClass(
      "flow-accuracy-badge",
      "flow-accuracy-warning",
    );
    expect(screen.getByText("Over-estimate · 36.8×")).toHaveAttribute(
      "title",
      expect.stringMatching(/estimate accuracy, not query speed/i),
    );
    expect(screen.getByText("Operator time · 0.01 ms")).toHaveAttribute(
      "title",
      "Time spent in this DuckDB operator",
    );
  });

  it("shows a specific verified aggregate title instead of the generic native label", () => {
    renderNode({
      id: "n0",
      operator: "count",
      nativeName: "HASH_GROUP_BY",
      semantic: {
        title: "Count non-null market values per group",
        summary: "COUNT(market) counts non-null values for each group.",
        inputLabel: "Rows in each group",
        outputLabel: "One count per group",
        sqlRange: { from: 27, to: 40 },
        conceptOnly: false,
      },
      source: null,
      estimatedRows: 2,
      actualRows: null,
      timingMs: null,
      rowsScanned: null,
      details: { Aggregates: "count(#1)" },
    });
    expect(screen.getByText("Count non-null market values per group")).toBeInTheDocument();
    expect(screen.getByText("HASH_GROUP_BY")).toBeInTheDocument();
  });

  it("shows actual output alone when DuckDB provides no estimate", () => {
    renderNode({
      id: "n0",
      operator: "result",
      nativeName: "QUERY",
      source: null,
      estimatedRows: null,
      actualRows: 2,
      timingMs: 0.01,
      rowsScanned: 734,
      details: {},
    });
    expect(screen.getByText("Actual output · 2 rows")).toBeInTheDocument();
    expect(screen.queryByText(/estimate/i)).not.toBeInTheDocument();
  });
});
