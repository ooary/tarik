import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { PlanViewState } from "./useQueryPlan";
import { QueryAnalysisWorkspace } from "./QueryAnalysisWorkspace";

const errorState: PlanViewState = {
  status: "error",
  plan: null,
  error: "catalog.missing: table not found",
  sql: "SELECT * FROM missing",
};

describe("QueryAnalysisWorkspace", () => {
  it("shows the immutable failed SQL snapshot and retries current SQL", () => {
    const onRun = vi.fn();
    render(
      <QueryAnalysisWorkspace
        currentSql="SELECT * FROM current_table"
        mode="profile"
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

  it("uses the same three-pane structure for an empty Estimate", () => {
    render(
      <QueryAnalysisWorkspace
        currentSql="SELECT 1"
        mode="explain"
        onClose={vi.fn()}
        onRun={vi.fn()}
        open
        state={{ status: "empty", plan: null, error: null, sql: null }}
      />,
    );
    expect(screen.getByRole("dialog", { name: "Estimate" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Planned SQL" })).toHaveTextContent(
      "SELECT 1",
    );
    expect(screen.getByRole("main", { name: "Estimated query graph" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Plan node inspector" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Build current SQL" })).toBeInTheDocument();
  });

  it("opens without execution when there is no profile state", () => {
    render(
      <QueryAnalysisWorkspace
        currentSql="SELECT 1"
        mode="profile"
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
