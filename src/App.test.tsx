import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import {
  cancelSourceOperation,
  clearCache,
  chooseDuckDbFile,
  chooseParquetFile,
  chooseSourceFile,
  closeProject,
  completeShutdown,
  createProject,
  createTable,
  dropCatalogObject,
  executeProfile,
  getActiveProject,
  getEngineStatus,
  getEngineResources,
  getLastSupportIncident,
  getRuntimeInfo,
  getTabExecution,
  getLogInfo,
  getProfileStatus,
  getWorkbenchPreferences,
  importSourceTable,
  inspectProjectCatalog,
  inspectSourceFile,
  linkParquetSource,
  listRecentProjects,
  listSources,
  loadQuerySession,
  openProject,
  registerShutdownReady,
  removeLinkedSource,
  removeProject,
  repairLinkedSource,
  renameProject,
  reopenRecentProject,
  revealLogDirectory,
  saveQuerySession,
  setWorkbenchPreferences,
} from "./lib/commands";

const shutdownListeners = new Set<(event: unknown) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((_event: string, handler: (event: unknown) => void) => {
    shutdownListeners.add(handler);
    return Promise.resolve(() => shutdownListeners.delete(handler));
  }),
}));

vi.mock("./lib/commands", () => ({
  cancelProfile: vi.fn(),
  cancelSourceOperation: vi.fn(),
  clearCache: vi.fn(),
  chooseDuckDbFile: vi.fn(),
  chooseParquetFile: vi.fn(),
  chooseSourceFile: vi.fn(),
  closeProject: vi.fn(),
  completeShutdown: vi.fn(),
  createProject: vi.fn(),
  createTable: vi.fn(),
  dropCatalogObject: vi.fn(),
  executeProfile: vi.fn(),
  getActiveProject: vi.fn(),
  getEngineStatus: vi.fn(),
  getEngineResources: vi.fn(),
  setEngineResources: vi.fn(),
  getLastSupportIncident: vi.fn(),
  getRuntimeInfo: vi.fn(),
  getTabExecution: vi.fn(),
  getLogInfo: vi.fn(),
  getProfileStatus: vi.fn(),
  getWorkbenchPreferences: vi.fn(),
  importSourceTable: vi.fn(),
  inspectProjectCatalog: vi.fn(),
  inspectSourceFile: vi.fn(),
  linkParquetSource: vi.fn(),
  listRecentProjects: vi.fn(),
  listSources: vi.fn(),
  loadQuerySession: vi.fn(),
  openProject: vi.fn(),
  registerShutdownReady: vi.fn(),
  removeLinkedSource: vi.fn(),
  removeProject: vi.fn(),
  repairLinkedSource: vi.fn(),
  renameProject: vi.fn(),
  reopenRecentProject: vi.fn(),
  releaseAllResults: vi.fn(),
  revealLogDirectory: vi.fn(),
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
    vi.mocked(getLastSupportIncident).mockResolvedValue(null);
    vi.mocked(getLogInfo).mockResolvedValue({
      directory: "/tmp/tarik/logs",
      activeFile: "/tmp/tarik/logs/tarik.log",
      maxFileBytes: 2 * 1024 * 1024,
      retainedFiles: 7,
      available: true,
    });
    vi.mocked(setWorkbenchPreferences).mockResolvedValue(undefined);
    vi.mocked(getActiveProject).mockResolvedValue(null);
    vi.mocked(getEngineStatus).mockResolvedValue({ state: "stopped", processId: null });
    vi.mocked(getEngineResources).mockResolvedValue({
      requested: { preset: "balanced", memoryLimitMib: 2048, threads: 2 },
      effective: null,
      state: "pending",
      logicalCpuCount: 4,
      physicalMemoryMib: 8192,
      minimumMemoryMib: 128,
      maximumMemoryMib: 262144,
      minimumThreads: 1,
      maximumThreads: 256,
    });
    vi.mocked(getTabExecution).mockResolvedValue(null);
    vi.mocked(executeProfile).mockResolvedValue({
      profileId: "profile-1",
      state: "queued",
      durationMs: 0,
      snapshot: null,
      error: null,
    });
    vi.mocked(getProfileStatus).mockResolvedValue(null);
    vi.mocked(inspectProjectCatalog).mockResolvedValue({ objects: [], columns: [] });
    vi.mocked(loadQuerySession).mockResolvedValue(null);
    vi.mocked(saveQuerySession).mockResolvedValue(undefined);
    vi.mocked(listRecentProjects).mockResolvedValue([]);
    vi.mocked(listSources).mockResolvedValue([]);
    vi.mocked(cancelSourceOperation).mockResolvedValue(true);
    vi.mocked(registerShutdownReady).mockResolvedValue(undefined);
    vi.mocked(completeShutdown).mockResolvedValue({
      phase: "complete",
      queriesCancelled: 0,
      exportsCancelled: 0,
      resultsReleased: 0,
      metadataCheckpointed: true,
      warnings: [],
    });
    vi.mocked(clearCache).mockResolvedValue({
      artifactsRemoved: 2,
      bytesRemoved: 4096,
      exportBackupsRestored: 0,
      warnings: [],
    });
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
    vi.mocked(createTable).mockResolvedValue(true);
    vi.mocked(dropCatalogObject).mockResolvedValue(true);
    runtimeInfoMock.mockResolvedValue({
      appName: "Tarik",
      appVersion: "0.1.0",
      rustTarget: "linux",
    });
  });

  it("suppresses the native WebView menu on unsupported chrome", () => {
    render(<App />);
    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    screen.getByRole("banner").dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("renders the workbench regions and runtime status", async () => {
    render(<App />);

    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Source explorer" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "SQL workspace" })).toBeInTheDocument();
    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
    expect(await screen.findByText("No DuckDB project open")).toBeInTheDocument();
  });

  it("uses semantic project actions and a truthful connection indicator", async () => {
    render(<App />);
    expect(screen.getByRole("button", { name: "New project" })).toHaveClass("project-new-button");
    expect(document.querySelector(".status-mark")).toHaveClass("status-mark-idle");
    expect(screen.queryByRole("button", { name: "Refresh catalog" })).not.toBeInTheDocument();
  });

  it("does not show connected when the engine-backed project command fails", async () => {
    vi.mocked(createProject).mockRejectedValue(new Error("engine handshake failed"));
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "New project" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), {
      target: { value: "Broken" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    expect(await screen.findByText("engine handshake failed")).toBeInTheDocument();
    expect(await screen.findByText("DuckDB connection failed")).toBeInTheDocument();
    expect(document.querySelector(".status-mark-failed")).toBeInTheDocument();
    expect(screen.queryByText(/Connected to/)).not.toBeInTheDocument();
  });

  it("creates and closes a real project through the typed commands", async () => {
    vi.mocked(getEngineStatus).mockResolvedValue({ state: "connected", processId: 42 });
    const prompt = vi.spyOn(window, "prompt");
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "New project" }));
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    expect(await screen.findByText("Connected to Local analysis")).toBeInTheDocument();
    expect(document.querySelector(".status-mark-connected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close project" })).toHaveClass(
      "project-close-button",
    );
    expect(createProject).toHaveBeenCalledWith("Local analysis");
    expect(prompt).not.toHaveBeenCalled();

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
    render(<App />);

    const row = await screen.findByTitle(/\/managed\/local-analysis\.duckdb/);
    fireEvent.contextMenu(row);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Project name" }), {
      target: { value: "Renamed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Rename project" }));
    await waitFor(() => expect(renameProject).toHaveBeenCalledWith("project-1", "Renamed"));

    fireEvent.contextMenu(await screen.findByTitle(/\/managed\/local-analysis\.duckdb/));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete project" }));
    expect(await screen.findByRole("dialog", { name: "Delete project?" })).toHaveTextContent(
      "/managed/local-analysis.duckdb",
    );
    fireEvent.click(screen.getByRole("button", { name: "Delete project" }));
    await waitFor(() => expect(removeProject).toHaveBeenCalledWith("project-1"));
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
    render(<App />);

    const row = await screen.findByTitle(/\/user\/warehouse\.duckdb/);
    fireEvent.contextMenu(row);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Forget project" }));

    expect(await screen.findByText(/external DuckDB file is preserved/)).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Forget project?" })).toHaveTextContent(
      "/user/warehouse.duckdb",
    );
    fireEvent.click(screen.getByRole("button", { name: "Forget project" }));
    await waitFor(() => expect(removeProject).toHaveBeenCalledWith("external-1"));
    expect(screen.queryByRole("menuitem", { name: "Delete project" })).not.toBeInTheDocument();
  });

  it("opens a populated DuckDB from the native selection boundary", async () => {
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
    expect(await screen.findByRole("dialog", { name: "Name this project" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Project name" })).toHaveValue("existing");
    fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), {
      target: { value: "Existing" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Open project" }));

    await waitFor(() => expect(screen.getByText("orders")).toBeInTheDocument());
    expect(screen.getByText("2 cols")).toBeInTheDocument();
    expect(screen.getByText("~1.2K rows")).toBeInTheDocument();
    expect(screen.getByTitle("Estimated: 1,200 rows")).toBeInTheDocument();
    expect(chooseDuckDbFile).toHaveBeenCalled();
    expect(openProject).toHaveBeenCalledWith("Existing", "/data/existing.duckdb");
  });

  it("opens Profile from supported Explorer menu and keyboard actions without scanning", async () => {
    vi.mocked(getActiveProject).mockResolvedValue({
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/project.duckdb",
    });
    vi.mocked(inspectProjectCatalog).mockResolvedValue({
      revision: "catalog-1",
      objects: [
        {
          database: "project",
          schema: "main",
          name: "orders",
          kind: "table",
          estimatedRowCount: 3,
        },
      ],
      columns: [
        {
          database: "project",
          schema: "main",
          object: "orders",
          name: "id",
          dataType: "BIGINT",
          position: 0,
          nullable: false,
        },
      ],
    });
    render(<App />);

    const row = await screen.findByRole("treeitem", { name: "orders table" });
    fireEvent.pointerDown(screen.getByRole("button", { name: "orders table actions" }), {
      button: 0,
      ctrlKey: false,
    });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Profile data" }));
    expect(await screen.findByRole("region", { name: "Profile orders" })).toBeInTheDocument();
    const preservedSqlWorkspace = document.querySelector<HTMLElement>(
      'section[aria-label="SQL workspace"]',
    );
    expect(preservedSqlWorkspace).toHaveAttribute("hidden");
    expect(executeProfile).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Back to query editor" }));
    await waitFor(() => expect(row).toHaveFocus());
    expect(screen.getByRole("region", { name: "SQL workspace" })).toBe(preservedSqlWorkspace);

    fireEvent.keyDown(row, { key: "p", altKey: true });
    expect(await screen.findByRole("region", { name: "Profile orders" })).toBeInTheDocument();
    expect(executeProfile).not.toHaveBeenCalled();
  });

  it("does not offer Profile for a missing linked source", async () => {
    vi.mocked(getActiveProject).mockResolvedValue({
      id: "project-1",
      name: "Local analysis",
      duckdbPath: "/data/project.duckdb",
    });
    vi.mocked(inspectProjectCatalog).mockResolvedValue({
      revision: "catalog-1",
      objects: [
        {
          database: "project",
          schema: "main",
          name: "orders_link",
          kind: "view",
          estimatedRowCount: null,
        },
      ],
      columns: [
        {
          database: "project",
          schema: "main",
          object: "orders_link",
          name: "id",
          dataType: "BIGINT",
          position: 0,
          nullable: true,
        },
      ],
    });
    vi.mocked(listSources).mockResolvedValue([
      {
        id: "source-2",
        projectId: "project-1",
        displayName: "orders_link",
        kind: "linked_parquet",
        state: "missing",
        sourcePath: "/data/missing.parquet",
        duckdbName: "orders_link",
        options: {},
        createdAt: "1",
        updatedAt: "1",
      },
    ]);
    render(<App />);

    await screen.findByRole("treeitem", { name: "orders_link view" });
    fireEvent.pointerDown(screen.getByRole("button", { name: "orders_link table actions" }), {
      button: 0,
      ctrlKey: false,
    });
    expect(await screen.findByRole("menuitem", { name: "Profile data" })).toHaveAttribute(
      "data-disabled",
    );
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
    render(<App />);

    await screen.findByTitle("main.orders");
    fireEvent.pointerDown(screen.getByRole("button", { name: "orders table actions" }), {
      button: 0,
      ctrlKey: false,
    });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete table" }));
    expect(
      await screen.findByText(/Original source preserved: \/data\/orders\.csv/),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Delete table" }));

    await waitFor(() =>
      expect(dropCatalogObject).toHaveBeenCalledWith(
        "project-1",
        "project",
        "main",
        "orders",
        "table",
      ),
    );
    await waitFor(() => expect(screen.queryByText("orders")).not.toBeInTheDocument());
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
    render(<App />);

    await screen.findByTitle("main.orders_link");
    fireEvent.pointerDown(screen.getByRole("button", { name: "orders_link table actions" }), {
      button: 0,
      ctrlKey: false,
    });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Remove link" }));
    expect(await screen.findByText(/original Parquet file is preserved/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Remove link" }));

    await waitFor(() => expect(removeLinkedSource).toHaveBeenCalledWith("source-2"));
    expect(dropCatalogObject).not.toHaveBeenCalled();
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

    await screen.findByTitle("/data/missing.parquet");
    fireEvent.pointerDown(screen.getByRole("button", { name: "orders source actions" }), {
      button: 0,
      ctrlKey: false,
    });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Locate replacement" }));

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

    await screen.findByTitle("analytics.order lines");
    fireEvent.pointerDown(screen.getByRole("button", { name: "order lines table actions" }), {
      button: 0,
      ctrlKey: false,
    });
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

  it("keeps Results as the only bottom output and orders analysis actions", () => {
    render(<App />);

    expect(screen.getByText("Results")).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Estimate" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Actual Flow" })).not.toBeInTheDocument();
    const toolbar = screen.getByRole("button", { name: /Query library/ }).parentElement!;
    expect(
      [...toolbar.querySelectorAll("button")].map((button) => button.textContent?.trim()),
    ).toEqual([
      expect.stringMatching(/^Run query/),
      "Save query",
      "Query library",
      "Estimate",
      "Actual Flow",
      "Export",
    ]);
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

  it("reveals the bounded support log directory from settings", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(await screen.findByText("7 files, up to 2 MiB each")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Reveal logs" }));

    expect(revealLogDirectory).toHaveBeenCalledWith();
  });

  it("clears only temporary result cache from settings", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(await screen.findByRole("button", { name: "Clear cache" }));

    await waitFor(() => expect(clearCache).toHaveBeenCalled());
    expect(screen.getByText("Removed 2 temporary artifacts.")).toBeInTheDocument();
    expect(screen.getByText(/completed exports are preserved/i)).toBeInTheDocument();
  });

  it("offers retry or forced quit when the latest draft cannot be saved", async () => {
    vi.mocked(completeShutdown)
      .mockRejectedValueOnce(new Error("shutdown.draft_flush_failed"))
      .mockResolvedValueOnce({
        phase: "complete",
        queriesCancelled: 0,
        exportsCancelled: 0,
        resultsReleased: 0,
        metadataCheckpointed: true,
        warnings: [],
      });
    render(<App />);
    await screen.findByText("No DuckDB project open");
    await waitFor(() => expect(registerShutdownReady).toHaveBeenCalled());

    // A draft save failure surfaces explicit recovery choices.
    vi.mocked(saveQuerySession).mockRejectedValue(new Error("disk full"));
    for (const handler of shutdownListeners) handler({ payload: undefined });
    await waitFor(() => expect(completeShutdown).toHaveBeenCalledWith(false));

    fireEvent.click(await screen.findByRole("button", { name: "Quit without latest changes" }));
    await waitFor(() => expect(completeShutdown).toHaveBeenCalledWith(true));
  });

  it("shows a browser-safe connection state when Tauri is unavailable", async () => {
    runtimeInfoMock.mockRejectedValue(new Error("Tauri unavailable"));

    render(<App />);

    expect(await screen.findByText("Starting Tarik")).toBeInTheDocument();
  });
});
