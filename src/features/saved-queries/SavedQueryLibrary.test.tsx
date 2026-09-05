import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  applyQueryHistoryRetention,
  clearQueryHistory,
  createQueryFolder,
  createSavedQuery,
  deleteQueryFolder,
  deleteSavedQuery,
  listQueryFolders,
  listQueryHistoryPage,
  listSavedQueries,
  renameQueryFolder,
  updateSavedQuery,
  type QueryFolder,
  type SavedQuery,
} from "../../lib/commands";
import { SavedQueryLibrary } from "./SavedQueryLibrary";

vi.mock("../../lib/commands", () => ({
  applyQueryHistoryRetention: vi.fn(),
  clearQueryHistory: vi.fn(),
  createQueryFolder: vi.fn(),
  createSavedQuery: vi.fn(),
  deleteQueryFolder: vi.fn(),
  deleteSavedQuery: vi.fn(),
  listQueryFolders: vi.fn(),
  listQueryHistoryPage: vi.fn(),
  listSavedQueries: vi.fn(),
  renameQueryFolder: vi.fn(),
  updateSavedQuery: vi.fn(),
}));

const folder: QueryFolder = {
  id: "f1",
  projectId: "p1",
  name: "Reporting",
  createdAt: "2026-01-01T00:00:00Z",
};
const saved: SavedQuery = {
  id: "q1",
  projectId: "p1",
  folderId: "f1",
  name: "Monthly revenue",
  sqlText: "SELECT sum(revenue) FROM orders",
  tags: ["finance", "monthly"],
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-02T00:00:00Z",
};

function renderLibrary(onOpenSql = vi.fn()) {
  render(
    <SavedQueryLibrary
      activeSql="SELECT * FROM orders"
      activeTitle="Orders"
      onOpenSql={onOpenSql}
      projectId="p1"
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /Query library/ }));
  return onOpenSql;
}

describe("SavedQueryLibrary", () => {
  beforeEach(() => {
    vi.mocked(listSavedQueries).mockResolvedValue([saved]);
    vi.mocked(listQueryFolders).mockResolvedValue([folder]);
    vi.mocked(listQueryHistoryPage).mockResolvedValue({
      entries: [
        {
          id: "h1",
          projectId: "p1",
          sqlText: "SELECT * FROM missing_orders",
          status: "failed",
          durationMs: 18,
          returnedRows: null,
          errorCode: "catalog.missing",
          errorMessage: "Table missing_orders does not exist",
          executedAt: "2026-01-03T12:00:00Z",
        },
      ],
      offset: 0,
      nextOffset: 25,
    });
    vi.mocked(applyQueryHistoryRetention).mockResolvedValue({ deleted: 3, remaining: 2 });
    vi.mocked(clearQueryHistory).mockResolvedValue({ deleted: 5, remaining: 0 });
    vi.mocked(createSavedQuery).mockResolvedValue({ ...saved, id: "q2", name: "Orders" });
    vi.mocked(updateSavedQuery).mockResolvedValue(saved);
    vi.mocked(createQueryFolder).mockResolvedValue(folder);
    vi.mocked(renameQueryFolder).mockResolvedValue(folder);
    vi.mocked(deleteQueryFolder).mockResolvedValue(true);
    vi.mocked(deleteSavedQuery).mockResolvedValue(true);
  });

  it("loads, searches, previews, and opens saved SQL without executing it", async () => {
    const onOpenSql = renderLibrary();
    expect(await screen.findAllByText("Monthly revenue")).toHaveLength(2);
    expect(screen.getByText("SELECT sum(revenue) FROM orders")).toBeInTheDocument();

    fireEvent.change(screen.getByRole("textbox", { name: "Search saved queries" }), {
      target: { value: "finance" },
    });
    await waitFor(() => expect(listSavedQueries).toHaveBeenCalledWith("p1", "finance"));

    fireEvent.click(screen.getByRole("button", { name: "Open in new tab" }));
    expect(onOpenSql).toHaveBeenCalledWith(saved.sqlText, saved.name);
  });

  it("saves the current editor SQL as a new record", async () => {
    renderLibrary();
    await screen.findAllByText("Monthly revenue");
    fireEvent.click(screen.getByRole("button", { name: /Save current/ }));
    fireEvent.click(screen.getByRole("button", { name: "Save as new" }));

    await waitFor(() =>
      expect(createSavedQuery).toHaveBeenCalledWith({
        projectId: "p1",
        folderId: null,
        name: "Orders",
        sqlText: "SELECT * FROM orders",
        tags: [],
      }),
    );
  });

  it("requires confirmation before replacing stored SQL", async () => {
    renderLibrary();
    await screen.findAllByText("Monthly revenue");
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("SQL"), { target: { value: "SELECT 2" } });
    fireEvent.click(screen.getByRole("button", { name: "Update saved query" }));
    expect(await screen.findByRole("dialog", { name: "Replace saved SQL?" })).toBeInTheDocument();
    expect(updateSavedQuery).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Replace saved SQL" }));
    await waitFor(() =>
      expect(updateSavedQuery).toHaveBeenCalledWith(
        "q1",
        expect.objectContaining({ sqlText: "SELECT 2" }),
      ),
    );
  });

  it("creates, renames, and deletes folders while preserving query intent", async () => {
    renderLibrary();
    await screen.findAllByText("Monthly revenue");
    fireEvent.click(screen.getByRole("button", { name: /New folder/ }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Folder name" }), {
      target: { value: "Finance" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create folder" }));
    await waitFor(() => expect(createQueryFolder).toHaveBeenCalledWith("p1", "Finance"));

    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Folder name" }), {
      target: { value: "BI" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Rename folder" }));
    await waitFor(() => expect(renameQueryFolder).toHaveBeenCalledWith("p1", "f1", "BI"));

    fireEvent.click(screen.getAllByRole("button", { name: "Delete" })[0]);
    expect(await screen.findByText(/kept and moved to Unfiled/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Delete folder" }));
    await waitFor(() => expect(deleteQueryFolder).toHaveBeenCalledWith("p1", "f1"));
  });

  it("filters and pages bounded history", async () => {
    renderLibrary();
    fireEvent.click(screen.getByRole("tab", { name: "History" }));
    expect(await screen.findAllByText("SELECT * FROM missing_orders")).toHaveLength(2);
    expect(screen.getByText("catalog.missing")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("History status"), { target: { value: "failed" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Search history" }), {
      target: { value: "missing" },
    });
    await waitFor(() =>
      expect(listQueryHistoryPage).toHaveBeenLastCalledWith(
        "p1",
        expect.objectContaining({ status: "failed", search: "missing", offset: 0, limit: 25 }),
      ),
    );

    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await waitFor(() =>
      expect(listQueryHistoryPage).toHaveBeenLastCalledWith(
        "p1",
        expect.objectContaining({ offset: 25, limit: 25 }),
      ),
    );
  });

  it("reopens historical SQL without executing it", async () => {
    const onOpenSql = renderLibrary();
    fireEvent.click(screen.getByRole("tab", { name: "History" }));
    await screen.findAllByText("SELECT * FROM missing_orders");
    fireEvent.click(screen.getByRole("button", { name: "Open in new tab" }));
    expect(onOpenSql).toHaveBeenCalledWith("SELECT * FROM missing_orders", "Failed query");
  });

  it("applies count and age retention only after confirmation", async () => {
    renderLibrary();
    fireEvent.click(screen.getByRole("tab", { name: "History" }));
    await screen.findAllByText("SELECT * FROM missing_orders");
    fireEvent.click(screen.getByRole("button", { name: "Retention" }));
    fireEvent.change(screen.getByLabelText("Keep newest entries"), { target: { value: "100" } });
    fireEvent.change(screen.getByLabelText("Maximum age in days"), { target: { value: "30" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply retention" }));
    expect(await screen.findByText(/permanently removes matching history/)).toBeInTheDocument();
    expect(applyQueryHistoryRetention).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Apply retention" }));
    await waitFor(() =>
      expect(applyQueryHistoryRetention).toHaveBeenCalledWith("p1", {
        maxCount: 100,
        maxAgeDays: 30,
      }),
    );
    expect(await screen.findByText("Deleted 3 entries. 2 remain.")).toBeInTheDocument();
  });

  it("clears project history without touching saved queries", async () => {
    renderLibrary();
    fireEvent.click(screen.getByRole("tab", { name: "History" }));
    await screen.findAllByText("SELECT * FROM missing_orders");
    fireEvent.click(screen.getByRole("button", { name: "Clear history" }));
    expect(await screen.findByText(/permanently removes all query history/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Clear history" }));
    await waitFor(() => expect(clearQueryHistory).toHaveBeenCalledWith("p1"));
    expect(await screen.findByText("Deleted 5 history entries.")).toBeInTheDocument();
    expect(deleteSavedQuery).not.toHaveBeenCalled();
  });

  it("deletes a selected saved query only after confirmation", async () => {
    renderLibrary();
    await screen.findAllByText("Monthly revenue");
    const deleteButtons = screen.getAllByRole("button", { name: "Delete" });
    fireEvent.click(deleteButtons[deleteButtons.length - 1]);
    expect(await screen.findByText(/removes the saved copy only/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Delete saved query" }));
    await waitFor(() => expect(deleteSavedQuery).toHaveBeenCalledWith("p1", "q1"));
  });
});
