import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { PlanViewState } from "./useQueryPlan";
import { ActualFlowWorkspace } from "./ActualFlowWorkspace";

const errorState: PlanViewState = {
  status: "error",
  plan: null,
  error: "catalog.missing: table not found",
  sql: "SELECT * FROM missing",
};

describe("ActualFlowWorkspace", () => {
  it("shows the immutable failed SQL snapshot and retries current SQL", () => {
    const onRun = vi.fn();
    render(
      <ActualFlowWorkspace
        currentSql="SELECT * FROM current_table"
        onClose={vi.fn()}
        onRun={onRun}
        open
        state={errorState}
      />,
    );
    expect(screen.getByRole("complementary", { name: "Profiled SQL" })).toHaveTextContent(
      "SELECT * FROM missing",
    );
    expect(screen.getByText("Editor SQL changed")).toBeInTheDocument();
    expect(screen.getByText("catalog.missing: table not found")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try current SQL again" }));
    expect(onRun).toHaveBeenCalledTimes(1);
  });

  it("opens without execution when there is no profile state", () => {
    render(
      <ActualFlowWorkspace
        currentSql="SELECT 1"
        onClose={vi.fn()}
        onRun={vi.fn()}
        open
        state={{ status: "empty", plan: null, error: null, sql: null }}
      />,
    );
    expect(screen.getByText("No actual flow yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run current SQL" })).toBeInTheDocument();
  });
});
