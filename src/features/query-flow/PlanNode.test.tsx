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
