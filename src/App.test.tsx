import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import {
  chooseDuckDbFile,
  closeProject,
  createProject,
  getActiveProject,
  getRuntimeInfo,
  getWorkbenchPreferences,
  inspectProjectCatalog,
  listRecentProjects,
  openProject,
  removeProject,
  renameProject,
  reopenRecentProject,
  setWorkbenchPreferences,
} from "./lib/commands";

vi.mock("./lib/commands", () => ({
  chooseDuckDbFile: vi.fn(),
  closeProject: vi.fn(),
  createProject: vi.fn(),
  getActiveProject: vi.fn(),
  getRuntimeInfo: vi.fn(),
  getWorkbenchPreferences: vi.fn(),
  inspectProjectCatalog: vi.fn(),
  listRecentProjects: vi.fn(),
  openProject: vi.fn(),
  removeProject: vi.fn(),
  renameProject: vi.fn(),
  reopenRecentProject: vi.fn(),
  setWorkbenchPreferences: vi.fn(),
}));

const runtimeInfoMock = vi.mocked(getRuntimeInfo);

describe("Tarik workbench shell", () => {
  beforeEach(() => {
    runtimeInfoMock.mockReset();
    vi.mocked(getWorkbenchPreferences).mockResolvedValue(null);
    vi.mocked(setWorkbenchPreferences).mockResolvedValue(undefined);
    vi.mocked(getActiveProject).mockResolvedValue(null);
    vi.mocked(inspectProjectCatalog).mockResolvedValue({ objects: [], columns: [] });
    vi.mocked(listRecentProjects).mockResolvedValue([]);
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

    fireEvent.click(await screen.findByRole("button", { name: "Rename" }));
    await waitFor(() => expect(renameProject).toHaveBeenCalledWith("project-1", "Renamed"));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
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

    fireEvent.click(await screen.findByRole("button", { name: "Forget" }));

    await waitFor(() => expect(removeProject).toHaveBeenCalledWith("external-1"));
    expect(confirm).toHaveBeenCalledWith(
      expect.stringContaining("external DuckDB file is preserved"),
    );
    expect(screen.queryByRole("button", { name: "Delete" })).not.toBeInTheDocument();
    confirm.mockRestore();
  });

  it("opens a populated DuckDB from the native selection boundary", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("Existing");
    vi.mocked(inspectProjectCatalog).mockResolvedValue({
      objects: [{ database: "existing", schema: "main", name: "orders", kind: "table" }],
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

    expect(await screen.findByText("orders")).toBeInTheDocument();
    expect(screen.getByText("2 cols")).toBeInTheDocument();
    expect(chooseDuckDbFile).toHaveBeenCalled();
    expect(openProject).toHaveBeenCalledWith("Existing", "/data/existing.duckdb");
  });

  it("collapses and expands the source explorer", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Collapse source explorer" }));
    expect(screen.getByRole("button", { name: "Expand source explorer" })).toBeInTheDocument();
  });

  it("switches result surfaces with accessible tabs", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("tab", { name: "Flow" }));
    expect(screen.getByText("Query flow is ready")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "Profile" }));
    expect(screen.getByText("Profile is ready after execution")).toBeInTheDocument();
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
