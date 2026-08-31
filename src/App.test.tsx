import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import {
  cancelSourceOperation,
  chooseDuckDbFile,
  chooseParquetFile,
  chooseSourceFile,
  closeProject,
  createProject,
  dropCatalogObject,
  getActiveProject,
  getRuntimeInfo,
  getWorkbenchPreferences,
  importSourceTable,
  inspectProjectCatalog,
  inspectSourceFile,
  linkParquetSource,
  listRecentProjects,
  listSources,
  loadQuerySession,
  openProject,
  removeLinkedSource,
  removeProject,
  repairLinkedSource,
  renameProject,
  reopenRecentProject,
  saveQuerySession,
  setWorkbenchPreferences,
} from "./lib/commands";

vi.mock("./lib/commands", () => ({
  cancelSourceOperation: vi.fn(),
  chooseDuckDbFile: vi.fn(),
  chooseParquetFile: vi.fn(),
  chooseSourceFile: vi.fn(),
  closeProject: vi.fn(),
  createProject: vi.fn(),
  dropCatalogObject: vi.fn(),
  getActiveProject: vi.fn(),
  getRuntimeInfo: vi.fn(),
  getWorkbenchPreferences: vi.fn(),
  importSourceTable: vi.fn(),
  inspectProjectCatalog: vi.fn(),
  inspectSourceFile: vi.fn(),
  linkParquetSource: vi.fn(),
  listRecentProjects: vi.fn(),
  listSources: vi.fn(),
  loadQuerySession: vi.fn(),
  openProject: vi.fn(),
  removeLinkedSource: vi.fn(),
  removeProject: vi.fn(),
  repairLinkedSource: vi.fn(),
  renameProject: vi.fn(),
  reopenRecentProject: vi.fn(),
  releaseAllResults: vi.fn(),
  saveQuerySession: vi.fn(),
  setWorkbenchPreferences: vi.fn(),
}));

const runtimeInfoMock = vi.mocked(getRuntimeInfo);

const sourceInspection = {
  path: "/data/orders.csv",
  format: "csv" as const,
  suggestedName: "orders",
  fileSizeBytes: 12_400,
  rowCount: 1_000,
  rowCountExact: true,
  columns: [{ name: "id", dataType: "BIGINT", nullable: true }],
  previewRows: [[1]],
  csvOptions: { delimiter: ",", hasHeader: true, nullValue: null, allVarchar: false },
  warnings: [],
};

describe("Tarik workbench shell", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    runtimeInfoMock.mockReset();
    vi.mocked(getWorkbenchPreferences).mockResolvedValue(null);
    vi.mocked(setWorkbenchPreferences).mockResolvedValue(undefined);
    vi.mocked(getActiveProject).mockResolvedValue(null);
    vi.mocked(inspectProjectCatalog).mockResolvedValue({ objects: [], columns: [] });
    vi.mocked(loadQuerySession).mockResolvedValue(null);
    vi.mocked(saveQuerySession).mockResolvedValue(undefined);
    vi.mocked(listRecentProjects).mockResolvedValue([]);
    vi.mocked(listSources).mockResolvedValue([]);
    vi.mocked(cancelSourceOperation).mockResolvedValue(true);
    vi.mocked(chooseSourceFile).mockResolvedValue("/data/orders.csv");
    vi.mocked(chooseParquetFile).mockResolvedValue("/data/replacement.parquet");
    vi.mocked(inspectSourceFile).mockResolvedValue(sourceInspection);
    vi.mocked(importSourceTable).mockResolvedValue({
      source: {
        id: "source-1",
        projectId: "project-1",
        displayName: "orders",
        kind: "duckdb_table",
        state: "ready",
        sourcePath: "/data/orders.csv",
        duckdbName: "orders",
        options: {},
        createdAt: "1",
        updatedAt: "1",
      },
      inspection: sourceInspection,
    });
    vi.mocked(linkParquetSource).mockResolvedValue({
      source: {
        id: "source-2",
        projectId: "project-1",
        displayName: "orders",
        kind: "linked_parquet",
        state: "ready",
        sourcePath: "/data/orders.parquet",
        duckdbName: "orders",
        options: {},
        createdAt: "1",
        updatedAt: "1",
      },
      inspection: { ...sourceInspection, format: "parquet", csvOptions: null },
    });
    vi.mocked(repairLinkedSource).mockResolvedValue({
      source: {
        id: "source-2",
        projectId: "project-1",
        displayName: "orders",
        kind: "linked_parquet",
        state: "ready",
        sourcePath: "/data/replacement.parquet",
        duckdbName: "orders",
        options: {},
        createdAt: "1",
        updatedAt: "2",
      },
      inspection: { ...sourceInspection, format: "parquet", csvOptions: null },
    });
    vi.mocked(removeLinkedSource).mockResolvedValue(true);
    vi.mocked(removeProject).mockResolvedValue("deleted");
    vi.mocked(renameProject).mockResolvedValue({
      id: "project-1",
      name: "Renamed",
      duckdbPath: "/data/renamed.duckdb",
      ownership: "managed",
      createdAt: "2026-01-01T00:00:00Z",
      lastOpenedAt: "2026-01-01T00:00:01Z",
    });
    vi.mocked(chooseDuckDbFile).mockResolvedValue("/data/existing.duckdb");
    vi.mocked(reopenRecentProject).mockResolvedValue({
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/local-analysis.duckdb",
    });
    vi.mocked(createProject).mockResolvedValue({
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/project.duckdb",
    });
    vi.mocked(openProject).mockResolvedValue({
      id: "project-2",
      name: "Existing",
      duckdbPath: "/data/existing.duckdb",
    });
    vi.mocked(closeProject).mockResolvedValue(true);
    vi.mocked(dropCatalogObject).mockResolvedValue(true);
    runtimeInfoMock.mockResolvedValue({
      appName: "Tarik",
      appVersion: "0.1.0",
      rustTarget: "linux",
    });
  });

  it("renders the workbench regions and runtime status", async () => {
    render(<App />);

    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Source explorer" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "SQL workspace" })).toBeInTheDocument();
    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
    expect(await screen.findByText("No DuckDB project open")).toBeInTheDocument();
  });

  it("creates and closes a real project through the typed commands", async () => {
    const prompt = vi.spyOn(window, "prompt").mockReturnValue("Local analysis");
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "New project" }));
    expect(await screen.findByText("Connected to Local analysis")).toBeInTheDocument();
    expect(createProject).toHaveBeenCalledWith("Local analysis");

    fireEvent.click(screen.getByRole("button", { name: "Close project" }));
    await waitFor(() => expect(closeProject).toHaveBeenCalled());
    prompt.mockRestore();
  });

  it("shows and reopens recent projects after close", async () => {
    vi.mocked(listRecentProjects).mockResolvedValue([
      {
        id: "project-1",
        name: "Local analysis",
        duckdbPath: "/data/local-analysis.duckdb",
        ownership: "managed",
        createdAt: "2026-01-01T00:00:00Z",
        lastOpenedAt: "2026-01-01T00:00:00Z",
      },
    ]);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: /Local analysis/ }));

    expect(await screen.findByText("Connected to Local analysis")).toBeInTheDocument();
    expect(reopenRecentProject).toHaveBeenCalledWith("project-1");
  });

  it("renames and deletes a managed recent project with exact confirmation", async () => {
    const recent = {
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/managed/local-analysis.duckdb",
      ownership: "managed" as const,
      createdAt: "2026-01-01T00:00:00Z",
      lastOpenedAt: "2026-01-01T00:00:00Z",
    };
    vi.mocked(listRecentProjects).mockResolvedValue([recent]);
    const prompt = vi.spyOn(window, "prompt").mockReturnValue("Renamed");
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<App />);

    const row = await screen.findByTitle(/\/managed\/local-analysis\.duckdb/);
    fireEvent.contextMenu(row);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename" }));
    await waitFor(() => expect(renameProject).toHaveBeenCalledWith("project-1", "Renamed"));
    fireEvent.contextMenu(row);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete project" }));
    await waitFor(() => expect(removeProject).toHaveBeenCalledWith("project-1"));
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("/managed/local-analysis.duckdb"));
    prompt.mockRestore();
    confirm.mockRestore();
  });

  it("forgets external project metadata while showing file preservation", async () => {
    const external = {
      id: "external-1",
      name: "Warehouse",
      duckdbPath: "/user/warehouse.duckdb",
      ownership: "external" as const,
      createdAt: "2026-01-01T00:00:00Z",
      lastOpenedAt: "2026-01-01T00:00:00Z",
    };
    vi.mocked(listRecentProjects).mockResolvedValue([external]);
    vi.mocked(removeProject).mockResolvedValue("forgotten");
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<App />);

    const row = await screen.findByTitle(/\/user\/warehouse\.duckdb/);
    fireEvent.contextMenu(row);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Forget project" }));

    await waitFor(() => expect(removeProject).toHaveBeenCalledWith("external-1"));
    expect(confirm).toHaveBeenCalledWith(
      expect.stringContaining("external DuckDB file is preserved"),
    );
    expect(screen.queryByRole("menuitem", { name: "Delete project" })).not.toBeInTheDocument();
    confirm.mockRestore();
  });

  it("opens a populated DuckDB from the native selection boundary", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("Existing");
    vi.mocked(inspectProjectCatalog).mockResolvedValue({
      objects: [
        {
          database: "existing",
          schema: "main",
          name: "orders",
          kind: "table",
          estimatedRowCount: 1_200,
        },
      ],
      columns: [
        {
          database: "existing",
          schema: "main",
          object: "orders",
          name: "id",
          dataType: "BIGINT",
          position: 0,
          nullable: false,
        },
        {
          database: "existing",
          schema: "main",
          object: "orders",
          name: "amount",
          dataType: "DOUBLE",
          position: 1,
          nullable: true,
        },
      ],
    });
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Open" }));

    await waitFor(() => expect(screen.getByText("orders")).toBeInTheDocument());
    expect(screen.getByText("2 cols")).toBeInTheDocument();
    expect(screen.getByText("~1.2K rows")).toBeInTheDocument();
    expect(screen.getByTitle("Estimated: 1,200 rows")).toBeInTheDocument();
    expect(chooseDuckDbFile).toHaveBeenCalled();
    expect(openProject).toHaveBeenCalledWith("Existing", "/data/existing.duckdb");
  });

  it("deletes a table from the project and refreshes Explorer", async () => {
    const activeProject = {
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/project.duckdb",
    };
    const catalogWithTable = {
      objects: [
        {
          database: "project",
          schema: "main",
          name: "orders",
          kind: "table" as const,
          estimatedRowCount: 3,
        },
      ],
      columns: [],
    };
    vi.mocked(getActiveProject).mockResolvedValue(activeProject);
    vi.mocked(inspectProjectCatalog)
      .mockResolvedValueOnce(catalogWithTable)
      .mockResolvedValueOnce({ objects: [], columns: [] });
    vi.mocked(listSources).mockResolvedValue([
      {
        id: "source-1",
        projectId: "project-1",
        displayName: "orders",
        kind: "duckdb_table",
        state: "ready",
        sourcePath: "/data/orders.csv",
        duckdbName: "orders",
        options: {},
        createdAt: "1",
        updatedAt: "1",
      },
    ]);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<App />);

    const row = await screen.findByTitle("main.orders");
    fireEvent.contextMenu(row.closest("button")!);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete table" }));

    await waitFor(() =>
      expect(dropCatalogObject).toHaveBeenCalledWith(
        "project-1",
        "project",
        "main",
        "orders",
        "table",
      ),
    );
    expect(confirm).toHaveBeenCalledWith(
      expect.stringContaining("The original source file is preserved:\n/data/orders.csv"),
    );
    await waitFor(() => expect(screen.queryByText("orders")).not.toBeInTheDocument());
    confirm.mockRestore();
  });

  it("uses Remove link for a linked Parquet catalog view", async () => {
    vi.mocked(getActiveProject).mockResolvedValue({
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/project.duckdb",
    });
    vi.mocked(inspectProjectCatalog).mockResolvedValue({
      objects: [
        {
          database: "project",
          schema: "main",
          name: "orders_link",
          kind: "view",
          estimatedRowCount: null,
        },
      ],
      columns: [],
    });
    vi.mocked(listSources).mockResolvedValue([
      {
        id: "source-2",
        projectId: "project-1",
        displayName: "orders_link",
        kind: "linked_parquet",
        state: "ready",
        sourcePath: "/data/orders.parquet",
        duckdbName: "orders_link",
        options: {},
        createdAt: "1",
        updatedAt: "1",
      },
    ]);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<App />);

    const row = await screen.findByTitle("main.orders_link");
    fireEvent.contextMenu(row.closest("button")!);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Remove link" }));

    await waitFor(() => expect(removeLinkedSource).toHaveBeenCalledWith("source-2"));
    expect(dropCatalogObject).not.toHaveBeenCalled();
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("The Parquet file is preserved"));
    confirm.mockRestore();
  });

  it("opens the CSV import wizard and imports confirmed options", async () => {
    vi.mocked(getActiveProject).mockResolvedValue({
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/project.duckdb",
    });
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "Import file" }));
    expect(await screen.findByRole("dialog", { name: "Add local source" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Import table" }));

    await waitFor(() => expect(importSourceTable).toHaveBeenCalled());
    expect(chooseSourceFile).toHaveBeenCalled();
    expect(inspectSourceFile).toHaveBeenCalledWith("/data/orders.csv");
  });

  it("repairs a missing linked source from its explorer row", async () => {
    const activeProject = {
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/project.duckdb",
    };
    vi.mocked(getActiveProject).mockResolvedValue(activeProject);
    vi.mocked(listSources).mockResolvedValue([
      {
        id: "source-2",
        projectId: "project-1",
        displayName: "orders",
        kind: "linked_parquet",
        state: "missing",
        sourcePath: "/data/missing.parquet",
        duckdbName: "orders",
        options: {},
        createdAt: "1",
        updatedAt: "1",
      },
    ]);
    render(<App />);

    fireEvent.click(await screen.findByTitle("/data/missing.parquet"));

    await waitFor(() =>
      expect(repairLinkedSource).toHaveBeenCalledWith("source-2", "/data/replacement.parquet"),
    );
  });

  it("prefers cached exact import rows over catalog estimates", async () => {
    vi.mocked(getActiveProject).mockResolvedValue({
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/project.duckdb",
    });
    vi.mocked(inspectProjectCatalog).mockResolvedValue({
      objects: [
        {
          database: "local",
          schema: "main",
          name: "orders",
          kind: "table",
          estimatedRowCount: 900,
        },
      ],
      columns: [
        {
          database: "local",
          schema: "main",
          object: "orders",
          name: "id",
          dataType: "BIGINT",
          position: 0,
          nullable: false,
        },
      ],
    });
    vi.mocked(listSources).mockResolvedValue([
      {
        id: "source-1",
        projectId: "project-1",
        displayName: "orders",
        kind: "duckdb_table",
        state: "ready",
        sourcePath: "/data/orders.csv",
        duckdbName: "orders",
        options: { rowCount: 1_000, rowCountExact: true },
        createdAt: "1",
        updatedAt: "1",
      },
    ]);
    render(<App />);

    expect(await screen.findByText("1K rows")).toBeInTheDocument();
    expect(screen.queryByText("~900 rows")).not.toBeInTheDocument();
    expect(screen.getByTitle("Exact cached: 1,000 rows")).toBeInTheDocument();
  });

  it("opens a safely quoted table preview from the source context menu", async () => {
    vi.mocked(getActiveProject).mockResolvedValue({
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/project.duckdb",
    });
    vi.mocked(inspectProjectCatalog).mockResolvedValue({
      objects: [
        {
          database: "local",
          schema: "analytics",
          name: "order lines",
          kind: "table",
          estimatedRowCount: 12,
        },
      ],
      columns: [],
    });
    render(<App />);

    const table = await screen.findByTitle("analytics.order lines");
    fireEvent.contextMenu(table.closest("button")!);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Preview rows" }));

    await waitFor(() =>
      expect(document.querySelector(".cm-content")).toHaveTextContent(
        'FROM "analytics"."order lines"',
      ),
    );
  });

  it("collapses and expands the source explorer", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Collapse source explorer" }));
    expect(screen.getByRole("button", { name: "Expand source explorer" })).toBeInTheDocument();
  });

  it("switches result surfaces with accessible tabs", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("tab", { name: "Flow" }));
    expect(screen.getByText("No query flow yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run Explain" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "Profile" }));
    expect(screen.getByText("No execution profile yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run Profile" })).toBeInTheDocument();
  });

  it("collapses and expands the bottom panel", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Collapse result panel" }));
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Expand result panel" })).toBeInTheDocument();
  });

  it("stores a selected theme through the settings repository", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(await screen.findByRole("button", { name: /Always use the dark theme/ }));

    await waitFor(() =>
      expect(setWorkbenchPreferences).toHaveBeenCalledWith(
        expect.objectContaining({ theme: "dark" }),
      ),
    );
  });

  it("shows a browser-safe connection state when Tauri is unavailable", async () => {
    runtimeInfoMock.mockRejectedValue(new Error("Tauri unavailable"));

    render(<App />);

    expect(await screen.findByText("Starting Tarik")).toBeInTheDocument();
  });
});
