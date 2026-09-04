import { createRef } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cancelQuery,
  executeQuery,
  getQueryStatus,
  getResultPage,
  explainQueryPlan,
  listQueryFolders,
  listQueryHistoryPage,
  listSavedQueries,
  loadQuerySession,
  saveQuerySession,
  validateQuery,
} from "../../lib/commands";
import { QueryWorkspace, type QueryWorkspaceHandle } from "./QueryWorkspace";

vi.mock("../../lib/commands", () => ({
  loadQuerySession: vi.fn(),
  saveQuerySession: vi.fn(),
  executeQuery: vi.fn(),
  getQueryStatus: vi.fn(),
  validateQuery: vi.fn(),
  explainQueryPlan: vi.fn(),
  listSavedQueries: vi.fn(),
  listQueryFolders: vi.fn(),
  listQueryHistoryPage: vi.fn(),
  createSavedQuery: vi.fn(),
  updateSavedQuery: vi.fn(),
  deleteSavedQuery: vi.fn(),
  createQueryFolder: vi.fn(),
  renameQueryFolder: vi.fn(),
  deleteQueryFolder: vi.fn(),
  cancelQuery: vi.fn(),
  forgetTabExecution: vi.fn(),
  getResultPage: vi.fn(),
  releaseResult: vi.fn(),
}));

const firstPage = {
  resultId: "res-1",
  offset: 0,
  rowTotal: 24318,
  rowTotalExact: true,
  columns: [
    { name: "country", logicalType: "string", nativeType: "Utf8", nullable: true },
    { name: "orders", logicalType: "integer", nativeType: "Int64", nullable: true },
  ],
  rows: [
    ["Singapore", 6842],
    ["Indonesia", 11204],
  ],
  truncatedCells: [],
  cached: false,
};

const runningView = {
  executionId: "exec-1",
  projectId: "p1",
  tabId: "",
  state: "running" as const,
  durationMs: 120,
  rowsProduced: 1000,
  rowsAffected: null,
  error: null,
  resultId: null,
  rowTotal: null,
};
const succeededView = {
  ...runningView,
  state: "succeeded" as const,
  durationMs: 1820,
  rowsProduced: 24318,
  resultId: "res-1",
  rowTotal: 24318,
};
const failedView = {
  ...runningView,
  state: "failed" as const,
  durationMs: 40,
  rowsProduced: null,
  error: { code: "sql.parse", message: 'syntax error at or near "FORM"' },
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
    vi.mocked(validateQuery).mockImplementation(async (_projectId, _sql, revision) => ({
      revision,
      diagnostics: [],
    }));
    vi.mocked(cancelQuery).mockResolvedValue({ ...runningView, tabId: "t1" });
    vi.mocked(getResultPage).mockResolvedValue({ ...firstPage });
    vi.mocked(listSavedQueries).mockResolvedValue([]);
    vi.mocked(listQueryFolders).mockResolvedValue([]);
    vi.mocked(listQueryHistoryPage).mockResolvedValue({
      entries: [],
      offset: 0,
      nextOffset: null,
    });
    vi.mocked(explainQueryPlan).mockResolvedValue({
      mode: "explain",
      nodes: [
        {
          id: "n0",
          operator: "scan",
          nativeName: "SEQ_SCAN",
          source: "fixture.main.orders",
          estimatedRows: 100,
          actualRows: null,
          timingMs: null,
          rowsScanned: null,
          details: {},
        },
      ],
      edges: [],
      rootIds: ["n0"],
      rawPlan: "[]",
      fallbackReason: null,
    });
  });

  it("adds tabs and exposes SQL insertion/preview actions", async () => {
    const ref = createRef<QueryWorkspaceHandle>();
    const { container } = render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
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

  it("opens saved SQL in a named new tab without executing it", async () => {
    vi.mocked(listSavedQueries).mockResolvedValue([
      {
        id: "q1",
        projectId: "p1",
        folderId: null,
        name: "Monthly revenue",
        sqlText: "SELECT sum(revenue) FROM orders",
        tags: ["finance"],
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
      },
    ]);
    const { container } = render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Query library/ }));
    await screen.findAllByText("Monthly revenue");
    fireEvent.click(screen.getByRole("button", { name: "Open in new tab" }));

    expect(await screen.findByRole("tab", { name: /Monthly revenue/ })).toBeInTheDocument();
    expect(container.querySelector(".cm-content")).toHaveTextContent("SELECT sum(revenue)");
    expect(executeQuery).not.toHaveBeenCalled();
  });

  it("reopens historical SQL in a new tab without execution", async () => {
    vi.mocked(listQueryHistoryPage).mockResolvedValue({
      entries: [
        {
          id: "h1",
          projectId: "p1",
          sqlText: "SELECT * FROM historical_orders",
          status: "succeeded",
          durationMs: 42,
          returnedRows: 3,
          errorCode: null,
          errorMessage: null,
          executedAt: "2026-01-01T00:00:00Z",
        },
      ],
      offset: 0,
      nextOffset: null,
    });
    const { container } = render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Query library/ }));
    fireEvent.click(screen.getByRole("tab", { name: "History" }));
    await screen.findAllByText("SELECT * FROM historical_orders");
    fireEvent.click(screen.getByRole("button", { name: "Open in new tab" }));

    expect(await screen.findByRole("tab", { name: /Succeeded query/ })).toBeInTheDocument();
    expect(container.querySelector(".cm-content")).toHaveTextContent("historical_orders");
    expect(executeQuery).not.toHaveBeenCalled();
  });

  it("opens Estimate in the same three-pane structure as Actual Flow", async () => {
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT * FROM orders"));

    fireEvent.click(screen.getByRole("button", { name: "Estimate" }));
    await waitFor(() =>
      expect(explainQueryPlan).toHaveBeenCalledWith("p1", "SELECT * FROM orders", "explain"),
    );
    expect(await screen.findByRole("dialog", { name: "Estimate" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Planned SQL" })).toHaveTextContent(
      "SELECT * FROM orders",
    );
    expect(screen.getByRole("main", { name: "Estimated query graph" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Plan node inspector" })).toBeInTheDocument();
    const node = await screen.findByText("Read data");
    expect(node).toBeInTheDocument();
    expect(document.querySelector(".flow-mode-label")).toHaveTextContent(
      "Estimate · DuckDB Explain",
    );
    expect(document.querySelector(".flow-mode-label")).toHaveTextContent(
      "Planned operations and row-count guesses—not query results.",
    );
    expect(screen.getByText("Select an operation")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Present flow from start" }));
    expect(await screen.findByText(/DuckDB reads rows from/)).toBeInTheDocument();
    expect(node.closest(".flow-node")).toHaveClass("flow-node-selected");
    expect(screen.getByText("Planning estimate, not result count")).toBeInTheDocument();
    expect(screen.getByText("DuckDB estimated output")).toBeInTheDocument();
    expect(screen.getByText("~100 rows")).toBeInTheDocument();
  });

  it("marks an old Estimate out of date and rebuilds current SQL explicitly", async () => {
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT * FROM orders"));
    fireEvent.click(screen.getByRole("button", { name: "Estimate" }));
    await screen.findByText("Read data");

    act(() => ref.current?.insertSql("WHERE id = 2"));
    expect(screen.getByText("Editor SQL changed")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Planned SQL" })).not.toHaveTextContent(
      "WHERE id = 2",
    );
    fireEvent.click(screen.getByRole("button", { name: "Build current SQL" }));
    await waitFor(() =>
      expect(explainQueryPlan).toHaveBeenLastCalledWith(
        "p1",
        "SELECT * FROM orders WHERE id = 2",
        "explain",
      ),
    );
  });

  it("closes the Estimate workspace with Escape and leaves Results intact", async () => {
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT * FROM orders"));
    fireEvent.click(screen.getByRole("button", { name: "Estimate" }));
    expect(await screen.findByRole("dialog", { name: "Estimate" })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Estimate" })).not.toBeInTheDocument(),
    );
    expect(screen.getByText("Results")).toBeInTheDocument();
  });

  it("opens Actual Flow as a dedicated three-pane workspace", async () => {
    vi.mocked(explainQueryPlan).mockResolvedValue({
      mode: "profile",
      nodes: [
        {
          id: "n0",
          operator: "scan",
          nativeName: "SEQ_SCAN",
          source: "fixture.main.orders",
          estimatedRows: 100,
          actualRows: 2,
          timingMs: 0.12,
          rowsScanned: 100,
          details: {},
        },
      ],
      edges: [],
      rootIds: ["n0"],
      rawPlan: "[]",
      fallbackReason: null,
    });
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT * FROM orders"));
    fireEvent.click(screen.getByRole("button", { name: "Actual Flow" }));

    expect(await screen.findByRole("dialog", { name: "Actual Flow" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Profiled SQL" })).toHaveTextContent(
      "SELECT * FROM orders",
    );
    expect(screen.getByRole("main", { name: "Actual execution graph" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Plan node inspector" })).toBeInTheDocument();
    const node = await screen.findByText("Read data");
    fireEvent.click(node);
    expect(
      screen.getByRole("complementary", { name: "Profiled SQL" }).querySelector("mark"),
    ).toHaveTextContent("orders");
    expect(screen.getByText("Operator time")).toBeInTheDocument();
  });

  it("highlights the mapped SQL range for a selected node and clears unmappable nodes", async () => {
    vi.mocked(explainQueryPlan).mockResolvedValue({
      mode: "explain",
      nodes: [
        {
          id: "n0",
          operator: "scan",
          nativeName: "SEQ_SCAN",
          source: "fixture.main.orders",
          estimatedRows: 100,
          actualRows: null,
          timingMs: null,
          rowsScanned: null,
          details: {},
        },
        {
          id: "n1",
          operator: "projection",
          nativeName: "PROJECTION",
          source: null,
          estimatedRows: 100,
          actualRows: null,
          timingMs: null,
          rowsScanned: null,
          details: {},
        },
      ],
      edges: [{ id: "e0", source: "n0", target: "n1" }],
      rootIds: ["n1"],
      rawPlan: "[]",
      fallbackReason: null,
    });
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT * FROM orders"));
    fireEvent.click(screen.getByRole("button", { name: "Estimate" }));

    const scanNode = await screen.findByText("Read data");
    fireEvent.click(scanNode);
    expect(
      screen.getByRole("complementary", { name: "Planned SQL" }).querySelector("mark"),
    ).toHaveTextContent("orders");

    fireEvent.click(screen.getByText("Return columns"));
    expect(
      await screen.findByText(/Return columns normally keeps the same row count as its input/),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("complementary", { name: "Planned SQL" }).querySelector("mark"),
    ).toBeNull();

    act(() => ref.current?.insertSql("changed after Estimate"));
    expect(screen.getByText("Editor SQL changed")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Planned SQL" })).not.toHaveTextContent(
      "changed after Estimate",
    );
  });

  it("runs Actual Flow without a warning for a clearly read-only query", async () => {
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT * FROM orders"));
    const confirm = vi.spyOn(window, "confirm");

    fireEvent.click(screen.getByRole("button", { name: "Actual Flow" }));
    await waitFor(() =>
      expect(explainQueryPlan).toHaveBeenCalledWith("p1", "SELECT * FROM orders", "profile"),
    );
    expect(confirm).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it("keeps the profiled SQL snapshot until Actual Flow explicitly reruns current SQL", async () => {
    vi.mocked(explainQueryPlan).mockResolvedValue({
      mode: "profile",
      nodes: [
        {
          id: "n0",
          operator: "scan",
          nativeName: "SEQ_SCAN",
          source: "fixture.main.orders",
          estimatedRows: 10,
          actualRows: 2,
          timingMs: 0.1,
          rowsScanned: 10,
          details: {},
        },
      ],
      edges: [],
      rootIds: ["n0"],
      rawPlan: "[]",
      fallbackReason: null,
    });
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT * FROM orders"));
    fireEvent.click(screen.getByRole("button", { name: "Actual Flow" }));
    await screen.findByText("Read data");

    act(() => ref.current?.insertSql("WHERE id = 2"));
    expect(screen.getByText("Editor SQL changed")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Profiled SQL" })).toHaveTextContent(
      "SELECT * FROM orders",
    );
    expect(screen.getByRole("complementary", { name: "Profiled SQL" })).not.toHaveTextContent(
      "WHERE id = 2",
    );

    fireEvent.click(screen.getByRole("button", { name: "Run current SQL" }));
    await waitFor(() =>
      expect(explainQueryPlan).toHaveBeenLastCalledWith(
        "p1",
        "SELECT * FROM orders WHERE id = 2",
        "profile",
      ),
    );
  });

  it("requires confirmation before Actual Flow executes potentially mutating SQL", async () => {
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("CREATE TABLE guarded AS SELECT 1"));
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const callsBeforeCancel = vi.mocked(explainQueryPlan).mock.calls.length;

    fireEvent.click(screen.getByRole("button", { name: "Actual Flow" }));
    expect(confirm).toHaveBeenCalledWith(expect.stringMatching(/executes this SQL.*may modify/s));
    expect(explainQueryPlan).toHaveBeenCalledTimes(callsBeforeCancel);

    confirm.mockReturnValue(true);
    fireEvent.click(screen.getByRole("button", { name: "Actual Flow" }));
    await waitFor(() =>
      expect(explainQueryPlan).toHaveBeenCalledWith(
        "p1",
        "CREATE TABLE guarded AS SELECT 1",
        "profile",
      ),
    );
    confirm.mockRestore();
  });

  it("shows raw fallback when structured plan parsing is unavailable", async () => {
    vi.mocked(explainQueryPlan).mockResolvedValue({
      mode: "explain",
      nodes: [],
      edges: [],
      rootIds: [],
      rawPlan: "raw future plan",
      fallbackReason: "unknown structured shape",
    });
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT 1"));
    fireEvent.click(screen.getByRole("button", { name: "Estimate" }));
    expect(await screen.findByText("Structured graph unavailable")).toBeInTheDocument();
    expect(screen.getByText("unknown structured shape")).toBeInTheDocument();
  });

  it("runs the active tab and shows the running state", async () => {
    vi.mocked(getQueryStatus).mockResolvedValue({ ...runningView, tabId: "t1" });
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );

    const planCallsBeforeRun = vi.mocked(explainQueryPlan).mock.calls.length;
    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    expect(screen.getByText("Results")).toBeInTheDocument();
    expect(explainQueryPlan).toHaveBeenCalledTimes(planCallsBeforeRun);
    await waitFor(() =>
      expect(executeQuery).toHaveBeenCalledWith("p1", expect.any(String), expect.any(String)),
    );
    expect(executeQuery).toHaveBeenCalledTimes(1);
    expect(
      await screen.findByText("Running", { selector: ".result-state strong" }),
    ).toBeInTheDocument();
    expect(await screen.findByText(/1,000 rows produced so far/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("refreshes project data once after a successful query", async () => {
    const onQuerySucceeded = vi.fn();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onQuerySucceeded={onQuerySucceeded}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    await waitFor(() => expect(onQuerySucceeded).toHaveBeenCalledTimes(1));
  });

  it("does not refresh project data after a failed query", async () => {
    const onQuerySucceeded = vi.fn();
    vi.mocked(getQueryStatus).mockResolvedValue({ ...failedView, tabId: "t1" });
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onQuerySucceeded={onQuerySucceeded}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    await screen.findByText("sql.parse");
    expect(onQuerySucceeded).not.toHaveBeenCalled();
  });

  it("renders the virtualized grid after a successful run", async () => {
    const { container } = render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    await waitFor(() => expect(getResultPage).toHaveBeenCalledWith("res-1", 0));
    expect(await screen.findByRole("grid", { name: "Query results" })).toBeInTheDocument();
    // Regression: the grid must show real headers and row cells, not only the
    // "Rows x-y of z" toolbar count.
    expect(await screen.findByText("country")).toBeInTheDocument();
    expect(await screen.findByText("orders")).toBeInTheDocument();
    expect(await screen.findByText("Singapore")).toBeInTheDocument();
    expect(await screen.findByText("6842")).toBeInTheDocument();
    expect(container.querySelector(".result-duration")).toHaveTextContent("Completed in 1.8s");
  });

  it("resizes result columns by pointer and keyboard", async () => {
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    const resizer = await screen.findByRole("separator", { name: "Resize country column" });
    expect(resizer).toHaveAttribute("aria-valuenow", "150");

    fireEvent.pointerDown(resizer, { clientX: 100 });
    fireEvent.pointerMove(document, { clientX: 180 });
    fireEvent.pointerUp(document);
    expect(resizer).toHaveAttribute("aria-valuenow", "230");

    fireEvent.keyDown(resizer, { key: "ArrowLeft" });
    expect(resizer).toHaveAttribute("aria-valuenow", "214");

    fireEvent.doubleClick(resizer);
    expect(resizer).toHaveAttribute("aria-valuenow", "150");
  });

  it("keeps the grid DOM bounded while browsing a large result", async () => {
    const pageWithRows = {
      ...firstPage,
      rows: Array.from({ length: 500 }, (_, index) => [index + 1, `label-${index}`]),
    };
    vi.mocked(getResultPage).mockResolvedValue(pageWithRows);
    const { container } = render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    await screen.findByRole("grid");
    await waitFor(() => {
      // Without layout measurement only the initial overscan window renders.
      expect(container.querySelectorAll("[role='row']").length).toBeLessThan(30);
    });
  });

  it("shows the structured SQL error when execution fails", async () => {
    vi.mocked(getQueryStatus).mockResolvedValue({ ...failedView, tabId: "t1" });
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Run query/ }));
    expect(await screen.findByText("sql.parse")).toBeInTheDocument();
    expect(await screen.findByText(/syntax error at or near "FORM"/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
  });

  it("shows current pre-run diagnostics and clears them immediately while editing", async () => {
    vi.mocked(validateQuery).mockImplementation(async (_projectId, sql, revision) => ({
      revision,
      diagnostics: sql.includes("missing")
        ? [
            {
              code: "sql.catalog",
              message: 'Table with name "missing" does not exist',
              severity: "error" as const,
              from: sql.indexOf("missing"),
              to: sql.indexOf("missing") + "missing".length,
            },
          ]
        : [],
    }));
    const ref = createRef<QueryWorkspaceHandle>();
    const executeCallsBefore = vi.mocked(executeQuery).mock.calls.length;
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT * FROM missing"));
    expect(await screen.findByText(/1 problem: Table with name/)).toBeInTheDocument();
    expect(document.querySelector(".cm-lintRange-error")).toBeInTheDocument();
    expect(vi.mocked(executeQuery).mock.calls).toHaveLength(executeCallsBefore);

    act(() => ref.current?.insertSql(" fixed"));
    expect(screen.queryByText(/1 problem:/)).not.toBeInTheDocument();
    expect(document.querySelector(".cm-lintRange-error")).toBeNull();
  });

  it("reports a clean pre-run check without promising runtime success", async () => {
    const ref = createRef<QueryWorkspaceHandle>();
    render(
      <QueryWorkspace
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );
    act(() => ref.current?.insertSql("SELECT 1"));
    const clean = await screen.findByText("No problems detected before execution");
    expect(clean).toHaveAttribute("title", "Runtime-only failures may still occur.");
    expect(screen.queryByText(/guaranteed/i)).not.toBeInTheDocument();
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
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
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
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
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
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
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
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        projectId="p1"
      />,
    );
    const tab = screen.getByRole("tab", { name: /Untitled/ });

    fireEvent.contextMenu(tab);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename" }));
    expect(screen.getByRole("tab", { name: /Revenue query/ })).toBeInTheDocument();

    fireEvent.contextMenu(screen.getByRole("tab", { name: /Revenue query/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Duplicate" }));
    expect(screen.getAllByRole("tab")).toHaveLength(2);

    fireEvent.contextMenu(screen.getByRole("tab", { name: /Revenue query copy/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Close" }));
    expect(screen.queryByRole("tab", { name: /Revenue query copy/ })).not.toBeInTheDocument();
  });
});
