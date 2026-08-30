import { createRef } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cancelQuery,
  executeQuery,
  getQueryStatus,
  loadQuerySession,
  saveQuerySession,
} from "../../lib/commands";
import { QueryWorkspace, type QueryWorkspaceHandle } from "./QueryWorkspace";

vi.mock("../../lib/commands", () => ({
  loadQuerySession: vi.fn(),
  saveQuerySession: vi.fn(),
  executeQuery: vi.fn(),
  getQueryStatus: vi.fn(),
  cancelQuery: vi.fn(),
  forgetTabExecution: vi.fn(),
}));

const runningView = {
  executionId: "exec-1",
  projectId: "p1",
  tabId: "",
  state: "running" as const,
  durationMs: 120,
  rowsProduced: 1000,
  rowsAffected: null,
  error: null,
};
const succeededView = {
  ...runningView,
  state: "succeeded" as const,
  durationMs: 1820,
  rowsProduced: 24318,
};
const failedView = {
  ...runningView,
  state: "failed" as const,
  durationMs: 40,
  rowsProduced: null,
  error: { code: "sql.parse", message: "syntax error at or near \"FORM\"" },
};

const catalog = {
  objects: [
    {
      database: "local",
      schema: "main",
      name: "orders",
      kind: "table" as const,
      estimatedRowCount: 100,
    },
  ],
  columns: [],
};

describe("QueryWorkspace", () => {
  beforeEach(() => {
    vi.mocked(loadQuerySession).mockResolvedValue(null);
    vi.mocked(saveQuerySession).mockResolvedValue(undefined);
    vi.mocked(executeQuery).mockResolvedValue({ ...runningView, tabId: "t1" });
    vi.mocked(getQueryStatus).mockResolvedValue({ ...succeededView, tabId: "t1" });
    vi.mocked(cancelQuery).mockResolvedValue({ ...runningView, tabId: "t1" });
  });

  it("adds tabs and exposes SQL insertion/preview actions", async () => {
    const ref = createRef<QueryWorkspaceHandle>();
    const { container } = render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "New query tab" }));
    expect(screen.getAllByRole("tab", { name: /Untitled/ })).toHaveLength(2);

    act(() => ref.current?.openPreview('SELECT * FROM "main"."orders" LIMIT 100;'));
    expect(screen.getAllByRole("tab", { name: /Untitled/ })).toHaveLength(3);
    expect(container.querySelector(".cm-content")).toHaveTextContent("SELECT *");

    act(() => ref.current?.insertSql('"main"."orders"'));
    expect(container.querySelector(".cm-content")).toHaveTextContent('"main"."orders"');
  });

  it("runs the active tab and shows the running state", async () => {
    vi.mocked(getQueryStatus).mockResolvedValue({ ...runningView, tabId: "t1" });
    const { container } = render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    expect(executeQuery).toHaveBeenCalledWith(
      "p1",
      expect.any(String),
      expect.any(String),
    );
    expect(await screen.findByText("Running", { selector: ".result-state strong" })).toBeInTheDocument();
    expect(await screen.findByText(/1,000 rows produced so far/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(container.querySelector(".tab-count")).toHaveTextContent("1,000");
  });

  it("shows the succeeded summary with the returned row count", async () => {
    const { container } = render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    expect(
      await screen.findByText(/Returned 24,318 rows/, { selector: ".result-state strong" }),
    ).toBeInTheDocument();
    expect(container.querySelector(".result-duration")).toHaveTextContent("Completed in 1.8s");
    expect(screen.queryByText("Copy visible rows")).not.toBeInTheDocument();
  });

  it("shows the structured SQL error when execution fails", async () => {
    vi.mocked(getQueryStatus).mockResolvedValue({ ...failedView, tabId: "t1" });
    render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    expect(await screen.findByText("sql.parse")).toBeInTheDocument();
    expect(
      await screen.findByText(/syntax error at or near "FORM"/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
  });

  it("shows the completion message for statements without a row set", async () => {
    vi.mocked(getQueryStatus).mockResolvedValue({
      ...succeededView,
      tabId: "t1",
      rowsProduced: null,
      rowsAffected: 3,
      durationMs: 30,
    });
    render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    expect(await screen.findByText("Statement completed")).toBeInTheDocument();
    expect(await screen.findByText("3 rows affected.")).toBeInTheDocument();
  });

  it("cancels a running query from the results panel", async () => {
    vi.mocked(getQueryStatus).mockResolvedValue({ ...runningView, tabId: "t1" });
    render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    const cancelButton = await screen.findByRole("button", { name: "Cancel" });
    fireEvent.click(cancelButton);
    await waitFor(() => expect(cancelQuery).toHaveBeenCalledWith("exec-1"));
  });

  it("shows the empty state before any execution", () => {
    render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
      />,
    );

    expect(screen.getByText("No results yet")).toBeInTheDocument();
    expect(screen.getByText("Run a query to see results here.")).toBeInTheDocument();
  });

  it("supports tab rename, duplicate, move, and close from context menu", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("Revenue query");
    render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
      />,
    );
    const tab = screen.getByRole("tab", { name: /Untitled/ });

    fireEvent.contextMenu(tab);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename" }));
    expect(screen.getByRole("tab", { name: /Revenue query/ })).toBeInTheDocument();

    fireEvent.contextMenu(screen.getByRole("tab", { name: /Revenue query/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Duplicate" }));
    expect(screen.getAllByRole("tab")).toHaveLength(5); // 2 query tabs + 3 output tabs

    fireEvent.contextMenu(screen.getByRole("tab", { name: /Revenue query copy/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Close" }));
    expect(screen.queryByRole("tab", { name: /Revenue query copy/ })).not.toBeInTheDocument();
  });
});
