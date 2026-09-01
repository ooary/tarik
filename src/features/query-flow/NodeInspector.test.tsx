import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { PlanNode } from "../../lib/commands";
import { NodeInspector } from "./NodeInspector";

const filter: PlanNode = {
  id: "n0",
  operator: "filter",
  nativeName: "FILTER",
  source: null,
  estimatedRows: 147,
  actualRows: 4,
  timingMs: 0.12,
  rowsScanned: 734,
  details: { Filters: "ProductId = 'SMD100' OR ProductId = 'IFN20GB'" },
};

describe("NodeInspector cardinality comparison", () => {
  it("explains a Profile ratio without calling it query performance", () => {
    render(<NodeInspector mode="profile" node={filter} />);

    expect(screen.getByText("Over-estimate · 36.8×")).toHaveClass("flow-accuracy-warning");
    expect(screen.getByText("DuckDB estimated output")).toBeInTheDocument();
    expect(screen.getByText("~147 rows")).toBeInTheDocument();
    expect(screen.getByText("Actual output")).toBeInTheDocument();
    expect(screen.getByText("4 rows", { selector: "dd" })).toBeInTheDocument();
    expect(
      screen.getByText(/measures estimate accuracy, not whether the query is fast/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Check operator time and rows scanned for performance/),
    ).toBeInTheDocument();
  });

  it("does not show a ratio in estimated-only Flow", () => {
    render(<NodeInspector mode="explain" node={{ ...filter, actualRows: null }} />);
    expect(screen.getByText("Planning estimate, not result count")).toBeInTheDocument();
    expect(screen.queryByText(/Over-estimate/)).not.toBeInTheDocument();
  });
});
