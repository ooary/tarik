import {
  BookmarksIcon,
  ClockCounterClockwiseIcon,
  FloppyDiskIcon,
  FolderPlusIcon,
  MagnifyingGlassIcon,
  PlusIcon,
  XIcon,
} from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useMemo, useState } from "react";
import {
  ConfirmationDialog,
  Dialog as UiDialog,
  Field,
  TextEntryDialog,
} from "../../components/ui";
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
  type HistoryStatus,
  type QueryFolder,
  type QueryHistoryEntry,
  type SavedQuery,
  type SavedQueryDraft,
} from "../../lib/commands";

interface SavedQueryLibraryProps {
  projectId: string;
  activeSql: string;
  activeTitle: string;
  onOpenSql: (sql: string, title: string) => void;
}

type LibraryView = "saved" | "history";

const HISTORY_PAGE_SIZE = 25;

type FormState = {
  id: string | null;
  originalSql: string | null;
  name: string;
  sqlText: string;
  tagsText: string;
  folderId: string;
};

type LibraryTextIntent =
  | { kind: "create-folder"; projectId: string; value: string }
  | { kind: "rename-folder"; projectId: string; folder: QueryFolder; value: string };

type SaveQuerySnapshot = {
  projectId: string;
  suggestedName: string;
  sqlText: string;
};

type DirectSaveState = {
  snapshot: SaveQuerySnapshot;
  name: string;
  folderId: string;
  creatingFolder: boolean;
  folderName: string;
};

type LibraryConfirmIntent =
  | { kind: "replace-sql"; projectId: string; form: FormState }
  | { kind: "delete-folder"; projectId: string; folder: QueryFolder }
  | {
      kind: "apply-retention";
      projectId: string;
      maxCount: number | null;
      maxAgeDays: number | null;
    }
  | { kind: "clear-history"; projectId: string }
  | { kind: "delete-query"; projectId: string; query: SavedQuery };

const emptyForm = (): FormState => ({
  id: null,
  originalSql: null,
  name: "",
  sqlText: "",
  tagsText: "",
  folderId: "",
});

export function SavedQueryLibrary({
  projectId,
  activeSql,
  activeTitle,
  onOpenSql,
}: SavedQueryLibraryProps) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<LibraryView>("saved");
  const [queries, setQueries] = useState<SavedQuery[]>([]);
  const [folders, setFolders] = useState<QueryFolder[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [form, setForm] = useState<FormState | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<QueryHistoryEntry[]>([]);
  const [historyStatus, setHistoryStatus] = useState<HistoryStatus | "">("");
  const [historySearch, setHistorySearch] = useState("");
  const [historyFrom, setHistoryFrom] = useState("");
  const [historyTo, setHistoryTo] = useState("");
  const [historyOffset, setHistoryOffset] = useState(0);
  const [historyNextOffset, setHistoryNextOffset] = useState<number | null>(null);
  const [historyRevision, setHistoryRevision] = useState(0);
  const [selectedHistoryId, setSelectedHistoryId] = useState<string | null>(null);
  const [retentionOpen, setRetentionOpen] = useState(false);
  const [maxCount, setMaxCount] = useState("500");
  const [maxAgeDays, setMaxAgeDays] = useState("90");
  const [retentionSummary, setRetentionSummary] = useState<string | null>(null);
  const [textIntent, setTextIntent] = useState<LibraryTextIntent | null>(null);
  const [confirmIntent, setConfirmIntent] = useState<LibraryConfirmIntent | null>(null);
  const [interactionBusy, setInteractionBusy] = useState(false);
  const [interactionError, setInteractionError] = useState<string | null>(null);
  const [directSave, setDirectSave] = useState<DirectSaveState | null>(null);
  const [directSaveBusy, setDirectSaveBusy] = useState(false);
  const [directSaveError, setDirectSaveError] = useState<string | null>(null);
  const [saveStatus, setSaveStatus] = useState<string | null>(null);
  const [libraryRevision, setLibraryRevision] = useState(0);
  const selected = queries.find((query) => query.id === selectedId) ?? null;
  const selectedHistory = history.find((entry) => entry.id === selectedHistoryId) ?? null;

  const grouped = useMemo(() => {
    const groups = new Map<string, SavedQuery[]>();
    for (const query of queries) {
      const key = query.folderId ?? "";
      groups.set(key, [...(groups.get(key) ?? []), query]);
    }
    return groups;
  }, [queries]);

  const refresh = async (term = search) => {
    if (!projectId) return;
    setLoading(true);
    setError(null);
    try {
      const [nextQueries, nextFolders] = await Promise.all([
        listSavedQueries(projectId, term.trim() || null),
        listQueryFolders(projectId),
      ]);
      setQueries(nextQueries);
      setFolders(nextFolders);
      setSelectedId((current) =>
        current && nextQueries.some((query) => query.id === current)
          ? current
          : (nextQueries[0]?.id ?? null),
      );
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!open || view !== "saved") return;
    const timeout = window.setTimeout(() => void refresh(search), search ? 180 : 0);
    return () => window.clearTimeout(timeout);
    // refresh intentionally follows current project/open/search/revision only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, view, projectId, search, libraryRevision]);

  useEffect(() => {
    if (!open || view !== "history" || !projectId) return;
    const timeout = window.setTimeout(
      async () => {
        setLoading(true);
        setError(null);
        try {
          const page = await listQueryHistoryPage(projectId, {
            status: historyStatus || null,
            search: historySearch.trim() || null,
            executedFrom: historyFrom ? new Date(`${historyFrom}T00:00:00`).toISOString() : null,
            executedTo: historyTo ? new Date(`${historyTo}T23:59:59.999`).toISOString() : null,
            offset: historyOffset,
            limit: HISTORY_PAGE_SIZE,
          });
          setHistory(page.entries);
          setHistoryNextOffset(page.nextOffset);
          setSelectedHistoryId((current) =>
            current && page.entries.some((entry) => entry.id === current)
              ? current
              : (page.entries[0]?.id ?? null),
          );
        } catch (cause) {
          setError(String(cause));
        } finally {
          setLoading(false);
        }
      },
      historySearch ? 180 : 0,
    );
    return () => window.clearTimeout(timeout);
  }, [
    open,
    view,
    projectId,
    historyStatus,
    historySearch,
    historyFrom,
    historyTo,
    historyOffset,
    historyRevision,
  ]);

  const openDirectSave = () => {
    if (!projectId || !activeSql.trim()) return;
    const snapshot: SaveQuerySnapshot = {
      projectId,
      suggestedName: activeTitle === "Untitled" ? "" : activeTitle,
      sqlText: activeSql,
    };
    setDirectSaveError(null);
    setSaveStatus(null);
    setDirectSave({
      snapshot,
      name: snapshot.suggestedName,
      folderId: "",
      creatingFolder: false,
      folderName: "",
    });
    void listQueryFolders(projectId)
      .then((nextFolders) => setFolders(nextFolders))
      .catch((cause) => setDirectSaveError(String(cause)));
  };

  const startCreate = () => {
    if (!open) {
      openDirectSave();
      return;
    }
    setError(null);
    setForm({
      ...emptyForm(),
      name: activeTitle === "Untitled" ? "" : activeTitle,
      sqlText: activeSql,
    });
  };

  async function createDirectSaveFolder() {
    if (!directSave || directSaveBusy) return;
    const name = directSave.folderName.trim();
    if (!name) return;
    setDirectSaveBusy(true);
    setDirectSaveError(null);
    try {
      if (projectId !== directSave.snapshot.projectId) {
        throw new Error("project.stale: The active project changed.");
      }
      const created = await createQueryFolder(projectId, name);
      const nextFolders = await listQueryFolders(projectId);
      setFolders(
        nextFolders.some((folder) => folder.id === created.id)
          ? nextFolders
          : [...nextFolders, created],
      );
      setDirectSave((current) =>
        current
          ? { ...current, folderId: created.id, creatingFolder: false, folderName: "" }
          : current,
      );
      setLibraryRevision((current) => current + 1);
    } catch (cause) {
      setDirectSaveError(String(cause));
    } finally {
      setDirectSaveBusy(false);
    }
  }

  async function submitDirectSave() {
    if (!directSave || directSaveBusy) return;
    const snapshot = directSave;
    if (!snapshot.name.trim() || !snapshot.snapshot.sqlText.trim()) return;
    setDirectSaveBusy(true);
    setDirectSaveError(null);
    try {
      if (projectId !== snapshot.snapshot.projectId) {
        throw new Error("project.stale: The active project changed.");
      }
      if (snapshot.folderId && !folders.some((folder) => folder.id === snapshot.folderId)) {
        throw new Error("query_folder.stale: The selected folder is no longer available.");
      }
      await createSavedQuery({
        projectId: snapshot.snapshot.projectId,
        folderId: snapshot.folderId || null,
        name: snapshot.name,
        sqlText: snapshot.snapshot.sqlText,
        tags: [],
      });
      const folder = folderName(folders, snapshot.folderId || null);
      setSaveStatus(`Saved “${snapshot.name.trim()}” in ${folder}.`);
      setDirectSave(null);
      setLibraryRevision((current) => current + 1);
    } catch (cause) {
      setDirectSaveError(String(cause));
    } finally {
      setDirectSaveBusy(false);
    }
  }

  const startEdit = (query: SavedQuery) => {
    setError(null);
    setForm({
      id: query.id,
      originalSql: query.sqlText,
      name: query.name,
      sqlText: query.sqlText,
      tagsText: query.tags.join(", "),
      folderId: query.folderId ?? "",
    });
  };

  const persistForm = async (snapshot: FormState) => {
    const draft: SavedQueryDraft = {
      projectId,
      folderId: snapshot.folderId || null,
      name: snapshot.name,
      sqlText: snapshot.sqlText,
      tags: snapshot.tagsText
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean),
    };
    setSaving(true);
    setError(null);
    try {
      const saved = snapshot.id
        ? await updateSavedQuery(snapshot.id, draft)
        : await createSavedQuery(draft);
      setForm(null);
      await refresh();
      setSelectedId(saved.id);
      setConfirmIntent(null);
    } catch (cause) {
      setError(String(cause));
      throw cause;
    } finally {
      setSaving(false);
    }
  };

  const save = async () => {
    if (!form) return;
    const snapshot = { ...form };
    if (snapshot.id && snapshot.originalSql !== snapshot.sqlText) {
      setInteractionError(null);
      setConfirmIntent({ kind: "replace-sql", projectId, form: snapshot });
      return;
    }
    try {
      await persistForm(snapshot);
    } catch {
      // persistForm retains the form and displays the existing library error.
    }
  };

  const addFolder = () => {
    setInteractionError(null);
    setTextIntent({ kind: "create-folder", projectId, value: "" });
  };

  const renameFolder = (folder: QueryFolder) => {
    setInteractionError(null);
    setTextIntent({ kind: "rename-folder", projectId, folder, value: folder.name });
  };

  const removeFolder = (folder: QueryFolder) => {
    setInteractionError(null);
    setConfirmIntent({ kind: "delete-folder", projectId, folder });
  };

  const applyRetention = () => {
    const count = maxCount.trim() ? Number.parseInt(maxCount, 10) : null;
    const days = maxAgeDays.trim() ? Number.parseInt(maxAgeDays, 10) : null;
    if (
      (count != null && (!Number.isInteger(count) || count < 0)) ||
      (days != null && (!Number.isInteger(days) || days < 0)) ||
      (count == null && days == null)
    ) {
      setError("Choose a non-negative max count, max age, or both.");
      return;
    }
    setInteractionError(null);
    setConfirmIntent({
      kind: "apply-retention",
      projectId,
      maxCount: count,
      maxAgeDays: days,
    });
  };

  const clearHistory = () => {
    setInteractionError(null);
    setConfirmIntent({ kind: "clear-history", projectId });
  };

  const removeQuery = (query: SavedQuery) => {
    setInteractionError(null);
    setConfirmIntent({ kind: "delete-query", projectId, query });
  };

  async function submitTextIntent() {
    if (!textIntent || interactionBusy) return;
    const intent = textIntent;
    const name = intent.value.trim();
    if (!name) return;
    setInteractionBusy(true);
    setInteractionError(null);
    try {
      if (projectId !== intent.projectId) {
        throw new Error("project.stale: The active project changed.");
      }
      if (intent.kind === "create-folder") {
        await createQueryFolder(projectId, name);
      } else {
        if (!folders.some((folder) => folder.id === intent.folder.id)) {
          throw new Error("query_folder.stale: This folder is no longer available.");
        }
        if (name === intent.folder.name) {
          setTextIntent(null);
          return;
        }
        await renameQueryFolder(projectId, intent.folder.id, name);
      }
      await refresh();
      setTextIntent(null);
    } catch (cause) {
      setInteractionError(String(cause));
    } finally {
      setInteractionBusy(false);
    }
  }

  async function submitConfirmation() {
    if (!confirmIntent || interactionBusy) return;
    const intent = confirmIntent;
    setInteractionBusy(true);
    setInteractionError(null);
    try {
      if (projectId !== intent.projectId) {
        throw new Error("project.stale: The active project changed.");
      }
      if (intent.kind === "replace-sql") {
        if (!queries.some((query) => query.id === intent.form.id)) {
          throw new Error("saved_query.stale: This saved query is no longer available.");
        }
        await persistForm(intent.form);
      } else if (intent.kind === "delete-folder") {
        if (!folders.some((folder) => folder.id === intent.folder.id)) {
          throw new Error("query_folder.stale: This folder is no longer available.");
        }
        await deleteQueryFolder(projectId, intent.folder.id);
        await refresh();
      } else if (intent.kind === "apply-retention") {
        const summary = await applyQueryHistoryRetention(projectId, {
          maxCount: intent.maxCount,
          maxAgeDays: intent.maxAgeDays,
        });
        setRetentionSummary(
          `Deleted ${summary.deleted.toLocaleString()} entries. ${summary.remaining.toLocaleString()} remain.`,
        );
        setHistoryOffset(0);
        setHistoryRevision((current) => current + 1);
        setRetentionOpen(false);
      } else if (intent.kind === "clear-history") {
        const summary = await clearQueryHistory(projectId);
        setHistory([]);
        setSelectedHistoryId(null);
        setHistoryNextOffset(null);
        setHistoryOffset(0);
        setHistoryRevision((current) => current + 1);
        setRetentionSummary(`Deleted ${summary.deleted.toLocaleString()} history entries.`);
      } else {
        if (!queries.some((query) => query.id === intent.query.id)) {
          throw new Error("saved_query.stale: This saved query is no longer available.");
        }
        await deleteSavedQuery(projectId, intent.query.id);
        setForm(null);
        await refresh();
      }
      setConfirmIntent(null);
    } catch (cause) {
      setInteractionError(String(cause));
    } finally {
      setInteractionBusy(false);
    }
  }

  const renderGroup = (
    label: string,
    folderId: string,
    rows: SavedQuery[],
    folder?: QueryFolder,
  ) => {
    if (rows.length === 0 && search) return null;
    return (
      <section className="saved-query-group" key={folderId || "unfiled"}>
        <header>
          <strong>{label}</strong>
          {folder && (
            <span>
              <button onClick={() => void renameFolder(folder)} type="button">
                Rename
              </button>
              <button onClick={() => void removeFolder(folder)} type="button">
                Delete
              </button>
            </span>
          )}
        </header>
        {rows.length === 0 ? (
          <span className="saved-query-group-empty">No saved queries.</span>
        ) : (
          rows.map((query) => (
            <button
              aria-pressed={selectedId === query.id}
              className="saved-query-row"
              key={query.id}
              onClick={() => {
                setSelectedId(query.id);
                setForm(null);
              }}
              type="button"
            >
              <strong>{query.name}</strong>
              <span>{query.tags.length > 0 ? query.tags.join(", ") : "No tags"}</span>
            </button>
          ))
        )}
      </section>
    );
  };

  return (
    <>
      <button
        className="toolbar-button"
        disabled={!projectId || !activeSql.trim()}
        onClick={openDirectSave}
        type="button"
      >
        <FloppyDiskIcon aria-hidden="true" size={14} /> Save query
      </button>
      <Dialog.Root
        onOpenChange={(nextOpen) => {
          setOpen(nextOpen);
          if (!nextOpen) {
            setForm(null);
            setError(null);
          }
        }}
        open={open}
      >
        <Dialog.Trigger asChild>
          <button className="toolbar-button" disabled={!projectId} type="button">
            <BookmarksIcon aria-hidden="true" size={14} /> Query library
          </button>
        </Dialog.Trigger>
        <Dialog.Portal>
          <Dialog.Overlay className="ui-dialog-overlay" />
          <Dialog.Content className="ui-dialog-content query-library-dialog">
            <header className="ui-dialog-header">
              <div>
                <Dialog.Title>Query library</Dialog.Title>
                <Dialog.Description>Saved SQL for this local project.</Dialog.Description>
              </div>
              <Dialog.Close aria-label="Close query library" className="icon-button">
                <XIcon aria-hidden="true" size={16} weight="bold" />
              </Dialog.Close>
            </header>
            <div aria-label="Query library view" className="query-library-tabs" role="tablist">
              <button
                aria-selected={view === "saved"}
                onClick={() => setView("saved")}
                role="tab"
                type="button"
              >
                <BookmarksIcon aria-hidden="true" size={14} /> Saved queries
              </button>
              <button
                aria-selected={view === "history"}
                onClick={() => setView("history")}
                role="tab"
                type="button"
              >
                <ClockCounterClockwiseIcon aria-hidden="true" size={14} /> History
              </button>
            </div>
            <div className="query-library-toolbar" hidden={view !== "saved"}>
              <label className="query-library-search">
                <MagnifyingGlassIcon aria-hidden="true" size={14} />
                <span className="sr-only">Search saved queries</span>
                <input
                  aria-label="Search saved queries"
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="Search name, SQL, or tag"
                  value={search}
                />
              </label>
              <button
                className="toolbar-button"
                disabled={!activeSql.trim()}
                onClick={startCreate}
                type="button"
              >
                <PlusIcon aria-hidden="true" size={14} /> Save current
              </button>
              <button className="toolbar-button" onClick={() => void addFolder()} type="button">
                <FolderPlusIcon aria-hidden="true" size={14} /> New folder
              </button>
            </div>
            <div
              className="query-library-toolbar history-filter-toolbar"
              hidden={view !== "history"}
            >
              <label className="query-library-search">
                <MagnifyingGlassIcon aria-hidden="true" size={14} />
                <span className="sr-only">Search history</span>
                <input
                  aria-label="Search history"
                  onChange={(event) => {
                    setHistorySearch(event.target.value);
                    setHistoryOffset(0);
                  }}
                  placeholder="Search SQL or error"
                  value={historySearch}
                />
              </label>
              <label className="history-filter-field">
                <span>Status</span>
                <select
                  aria-label="History status"
                  onChange={(event) => {
                    setHistoryStatus(event.target.value as HistoryStatus | "");
                    setHistoryOffset(0);
                  }}
                  value={historyStatus}
                >
                  <option value="">All</option>
                  <option value="succeeded">Succeeded</option>
                  <option value="failed">Failed</option>
                  <option value="cancelled">Cancelled</option>
                </select>
              </label>
              <label className="history-filter-field">
                <span>From</span>
                <input
                  aria-label="History from date"
                  onChange={(event) => {
                    setHistoryFrom(event.target.value);
                    setHistoryOffset(0);
                  }}
                  type="date"
                  value={historyFrom}
                />
              </label>
              <label className="history-filter-field">
                <span>To</span>
                <input
                  aria-label="History to date"
                  onChange={(event) => {
                    setHistoryTo(event.target.value);
                    setHistoryOffset(0);
                  }}
                  type="date"
                  value={historyTo}
                />
              </label>
              <button
                aria-expanded={retentionOpen}
                className="toolbar-button"
                onClick={() => setRetentionOpen((current) => !current)}
                type="button"
              >
                Retention
              </button>
              <button
                className="toolbar-button danger-button"
                onClick={() => void clearHistory()}
                type="button"
              >
                Clear history
              </button>
            </div>
            {view === "history" && retentionOpen && (
              <div className="history-retention-panel">
                <Field
                  label="Keep newest entries"
                  min="0"
                  onChange={(event) => setMaxCount(event.target.value)}
                  type="number"
                  value={maxCount}
                />
                <Field
                  label="Maximum age in days"
                  min="0"
                  onChange={(event) => setMaxAgeDays(event.target.value)}
                  type="number"
                  value={maxAgeDays}
                />
                <span>
                  Leave one field blank to apply only the other. Saved queries and editor drafts are
                  never removed.
                </span>
                <button
                  className="run-button"
                  disabled={loading}
                  onClick={() => void applyRetention()}
                  type="button"
                >
                  Apply retention
                </button>
              </div>
            )}
            {view === "history" && retentionSummary && (
              <div className="history-retention-summary" role="status">
                {retentionSummary}
              </div>
            )}
            {error && (
              <div className="ui-inline-error" role="alert">
                <strong>Query library error</strong>
                <span>{error}</span>
              </div>
            )}
            <div className="query-library-body" hidden={view !== "saved"}>
              <div className="saved-query-list" aria-label="Saved queries">
                {loading ? (
                  <div className="saved-query-loading" role="status">
                    Loading saved queries
                  </div>
                ) : queries.length === 0 && folders.length === 0 && !search ? (
                  <div className="saved-query-empty">
                    <strong>No saved queries or folders yet</strong>
                    <span>Save the active editor SQL or create a folder for this project.</span>
                  </div>
                ) : (
                  <>
                    {queries.length > 0 || search
                      ? renderGroup("Unfiled", "", grouped.get("") ?? [])
                      : null}
                    {folders.map((folder) =>
                      renderGroup(folder.name, folder.id, grouped.get(folder.id) ?? [], folder),
                    )}
                    {search && queries.length === 0 && (
                      <div className="saved-query-empty saved-query-search-empty">
                        <strong>No matching saved queries</strong>
                        <span>Folders remain visible; clear the search to see all saved SQL.</span>
                      </div>
                    )}
                  </>
                )}
              </div>
              <div className="saved-query-detail">
                {form ? (
                  <>
                    <h3>{form.id ? "Edit saved query" : "Save current as new"}</h3>
                    <Field
                      autoFocus
                      label="Name"
                      onChange={(event) => setForm({ ...form, name: event.target.value })}
                      value={form.name}
                    />
                    <label className="saved-query-select">
                      <span>Folder</span>
                      <select
                        onChange={(event) => setForm({ ...form, folderId: event.target.value })}
                        value={form.folderId}
                      >
                        <option value="">Unfiled</option>
                        {folders.map((folder) => (
                          <option key={folder.id} value={folder.id}>
                            {folder.name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <Field
                      hint="Comma-separated, for example finance, monthly"
                      label="Tags"
                      onChange={(event) => setForm({ ...form, tagsText: event.target.value })}
                      value={form.tagsText}
                    />
                    <label className="saved-query-sql-field">
                      <span>SQL</span>
                      <textarea
                        onChange={(event) => setForm({ ...form, sqlText: event.target.value })}
                        rows={10}
                        value={form.sqlText}
                      />
                    </label>
                    <div className="saved-query-actions">
                      <button
                        className="toolbar-button"
                        disabled={saving}
                        onClick={() => setForm(null)}
                        type="button"
                      >
                        Cancel
                      </button>
                      <button
                        className="run-button"
                        disabled={saving || !form.name.trim() || !form.sqlText.trim()}
                        onClick={() => void save()}
                        type="button"
                      >
                        {saving ? "Saving" : form.id ? "Update saved query" : "Save as new"}
                      </button>
                    </div>
                  </>
                ) : selected ? (
                  <>
                    <div className="saved-query-detail-heading">
                      <div>
                        <h3>{selected.name}</h3>
                        <span>{folderName(folders, selected.folderId)}</span>
                      </div>
                      <span>{selected.tags.join(", ") || "No tags"}</span>
                    </div>
                    <pre>{selected.sqlText}</pre>
                    <small>Updated {formatTimestamp(selected.updatedAt)}</small>
                    <div className="saved-query-actions">
                      <button
                        className="toolbar-button"
                        onClick={() => startEdit(selected)}
                        type="button"
                      >
                        Edit
                      </button>
                      <button
                        className="toolbar-button danger-button"
                        onClick={() => void removeQuery(selected)}
                        type="button"
                      >
                        Delete
                      </button>
                      <Dialog.Close asChild>
                        <button
                          className="run-button"
                          onClick={() => onOpenSql(selected.sqlText, selected.name)}
                          type="button"
                        >
                          Open in new tab
                        </button>
                      </Dialog.Close>
                    </div>
                  </>
                ) : (
                  <div className="saved-query-empty">
                    <strong>Select a saved query</strong>
                    <span>Review its SQL before opening it in the editor.</span>
                  </div>
                )}
              </div>
            </div>
            <div className="query-library-body" hidden={view !== "history"}>
              <div aria-label="Query history" className="saved-query-list history-list">
                {loading ? (
                  <div className="saved-query-loading" role="status">
                    Loading query history
                  </div>
                ) : history.length === 0 ? (
                  <div className="saved-query-empty">
                    <strong>No matching history</strong>
                    <span>
                      Terminal query runs appear here once they succeed, fail, or are cancelled.
                    </span>
                  </div>
                ) : (
                  history.map((entry) => (
                    <button
                      aria-pressed={selectedHistoryId === entry.id}
                      className="saved-query-row history-row"
                      key={entry.id}
                      onClick={() => setSelectedHistoryId(entry.id)}
                      type="button"
                    >
                      <span className={`history-status history-status-${entry.status}`}>
                        {entry.status}
                      </span>
                      <strong>{firstSqlLine(entry.sqlText)}</strong>
                      <span>{formatTimestamp(entry.executedAt)}</span>
                    </button>
                  ))
                )}
                <div className="history-pagination">
                  <button
                    className="toolbar-button"
                    disabled={historyOffset === 0 || loading}
                    onClick={() => setHistoryOffset(Math.max(0, historyOffset - HISTORY_PAGE_SIZE))}
                    type="button"
                  >
                    Previous
                  </button>
                  <span>
                    Rows {historyOffset + (history.length > 0 ? 1 : 0)}-
                    {historyOffset + history.length}
                  </span>
                  <button
                    className="toolbar-button"
                    disabled={historyNextOffset == null || loading}
                    onClick={() => historyNextOffset != null && setHistoryOffset(historyNextOffset)}
                    type="button"
                  >
                    Next
                  </button>
                </div>
              </div>
              <div className="saved-query-detail history-detail">
                {selectedHistory ? (
                  <>
                    <div className="saved-query-detail-heading">
                      <div>
                        <h3>{historyTitle(selectedHistory)}</h3>
                        <span>{formatTimestamp(selectedHistory.executedAt)}</span>
                      </div>
                      <span>{formatHistoryMetrics(selectedHistory)}</span>
                    </div>
                    <pre>{selectedHistory.sqlText}</pre>
                    {selectedHistory.errorMessage && (
                      <div className="ui-inline-error">
                        <strong>{selectedHistory.errorCode ?? "Query failed"}</strong>
                        <span>{selectedHistory.errorMessage}</span>
                      </div>
                    )}
                    <div className="saved-query-actions">
                      <Dialog.Close asChild>
                        <button
                          className="run-button"
                          onClick={() =>
                            onOpenSql(selectedHistory.sqlText, historyTitle(selectedHistory))
                          }
                          type="button"
                        >
                          Open in new tab
                        </button>
                      </Dialog.Close>
                    </div>
                  </>
                ) : (
                  <div className="saved-query-empty">
                    <strong>Select a historical execution</strong>
                    <span>Review its SQL and terminal outcome before reopening it.</span>
                  </div>
                )}
              </div>
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
      {directSave && (
        <UiDialog
          busy={directSaveBusy}
          description="Save an immutable copy of the active editor SQL. This does not run the query."
          onOpenChange={(nextOpen) => {
            if (!nextOpen) {
              setDirectSave(null);
              setDirectSaveError(null);
            }
          }}
          open
          title="Save query"
        >
          <form
            className="ui-dialog-form direct-save-form"
            onSubmit={(event) => {
              event.preventDefault();
              void submitDirectSave();
            }}
          >
            <Field
              autoFocus
              disabled={directSaveBusy}
              error={!directSave.name.trim() ? "Enter a query name." : undefined}
              label="Query name"
              onChange={(event) =>
                setDirectSave((current) =>
                  current ? { ...current, name: event.target.value } : current,
                )
              }
              value={directSave.name}
            />
            <label className="saved-query-select">
              <span>Folder</span>
              <select
                disabled={directSaveBusy}
                onChange={(event) =>
                  setDirectSave((current) =>
                    current ? { ...current, folderId: event.target.value } : current,
                  )
                }
                value={directSave.folderId}
              >
                <option value="">Unfiled</option>
                {folders.map((folder) => (
                  <option key={folder.id} value={folder.id}>
                    {folder.name}
                  </option>
                ))}
              </select>
            </label>
            {directSave.creatingFolder ? (
              <div className="direct-save-new-folder">
                <Field
                  disabled={directSaveBusy}
                  label="New folder name"
                  onChange={(event) =>
                    setDirectSave((current) =>
                      current ? { ...current, folderName: event.target.value } : current,
                    )
                  }
                  value={directSave.folderName}
                />
                <button
                  className="toolbar-button"
                  disabled={directSaveBusy || !directSave.folderName.trim()}
                  onClick={() => void createDirectSaveFolder()}
                  type="button"
                >
                  Create folder
                </button>
              </div>
            ) : (
              <button
                className="text-button direct-save-folder-toggle"
                onClick={() =>
                  setDirectSave((current) =>
                    current ? { ...current, creatingFolder: true } : current,
                  )
                }
                type="button"
              >
                <FolderPlusIcon aria-hidden="true" size={14} /> Create a folder
              </button>
            )}
            <section aria-label="SQL snapshot" className="direct-save-snapshot">
              <span>
                SQL snapshot · {directSave.snapshot.sqlText.length.toLocaleString()} characters
              </span>
              <code>{boundedSqlPreview(directSave.snapshot.sqlText)}</code>
            </section>
            {directSaveError && (
              <div className="ui-inline-error" role="alert">
                <strong>Query could not be saved</strong>
                <span>{directSaveError}</span>
              </div>
            )}
            <div className="ui-dialog-actions">
              <button
                className="toolbar-button"
                disabled={directSaveBusy}
                onClick={() => setDirectSave(null)}
                type="button"
              >
                Cancel
              </button>
              <button
                className="run-button"
                disabled={directSaveBusy || !directSave.name.trim()}
                type="submit"
              >
                {directSaveBusy ? "Saving…" : "Save query"}
              </button>
            </div>
          </form>
        </UiDialog>
      )}
      {saveStatus && (
        <span className="sr-only" role="status">
          {saveStatus}
        </span>
      )}
      {textIntent && (
        <TextEntryDialog
          busy={interactionBusy}
          description={
            textIntent.kind === "create-folder"
              ? "Create a folder for saved SQL in this local project."
              : `Choose a new name for “${textIntent.folder.name}”.`
          }
          label="Folder name"
          onOpenChange={(nextOpen) => {
            if (!nextOpen) {
              setTextIntent(null);
              setInteractionError(null);
            }
          }}
          onSubmit={submitTextIntent}
          onValueChange={(value) =>
            setTextIntent((current) => (current ? { ...current, value } : current))
          }
          open
          operationError={interactionError}
          submitLabel={textIntent.kind === "create-folder" ? "Create folder" : "Rename folder"}
          title={textIntent.kind === "create-folder" ? "New folder" : "Rename folder"}
          value={textIntent.value}
        />
      )}
      {confirmIntent && (
        <ConfirmationDialog
          busy={interactionBusy || saving || loading}
          confirmLabel={libraryConfirmationLabel(confirmIntent)}
          description={libraryConfirmationDescription(confirmIntent)}
          detail={libraryConfirmationDetail(confirmIntent)}
          error={interactionError}
          onConfirm={submitConfirmation}
          onOpenChange={(nextOpen) => {
            if (!nextOpen) {
              setConfirmIntent(null);
              setInteractionError(null);
            }
          }}
          open
          title={`${libraryConfirmationLabel(confirmIntent)}?`}
          tone="destructive"
        />
      )}
    </>
  );
}

function boundedSqlPreview(sql: string): string {
  const normalized = sql.trim();
  return normalized.length > 1_000 ? `${normalized.slice(0, 1_000)}…` : normalized;
}

function libraryConfirmationLabel(intent: LibraryConfirmIntent): string {
  if (intent.kind === "replace-sql") return "Replace saved SQL";
  if (intent.kind === "delete-folder") return "Delete folder";
  if (intent.kind === "apply-retention") return "Apply retention";
  if (intent.kind === "clear-history") return "Clear history";
  return "Delete saved query";
}

function libraryConfirmationDescription(intent: LibraryConfirmIntent): string {
  if (intent.kind === "replace-sql") {
    return "The previous saved SQL will be overwritten. The editor draft is not changed.";
  }
  if (intent.kind === "delete-folder") {
    return "Saved queries in this folder are kept and moved to Unfiled.";
  }
  if (intent.kind === "apply-retention") {
    return "This permanently removes matching history for this project. Saved queries and drafts are kept.";
  }
  if (intent.kind === "clear-history") {
    return "This permanently removes all query history for this project. Saved queries and drafts are kept.";
  }
  return "This removes the saved copy only. Editor tabs are not changed.";
}

function libraryConfirmationDetail(intent: LibraryConfirmIntent): string {
  if (intent.kind === "replace-sql") return intent.form.name.trim() || "Saved query";
  if (intent.kind === "delete-folder") return intent.folder.name;
  if (intent.kind === "apply-retention") {
    const parts = [];
    if (intent.maxCount != null) parts.push(`Keep newest ${intent.maxCount.toLocaleString()}`);
    if (intent.maxAgeDays != null) parts.push(`Keep ${intent.maxAgeDays.toLocaleString()} days`);
    return parts.join(" · ");
  }
  if (intent.kind === "clear-history") return "All history in this project";
  return intent.query.name;
}

function firstSqlLine(sql: string): string {
  return sql.split(/\r?\n/, 1)[0]?.trim() || "Empty SQL";
}

function historyTitle(entry: QueryHistoryEntry): string {
  const label = entry.status[0].toUpperCase() + entry.status.slice(1);
  return `${label} query`;
}

function formatHistoryMetrics(entry: QueryHistoryEntry): string {
  const metrics = [];
  if (entry.durationMs != null) metrics.push(formatDuration(entry.durationMs));
  if (entry.returnedRows != null) metrics.push(`${entry.returnedRows.toLocaleString()} rows`);
  return metrics.join(" · ") || entry.status;
}

function formatDuration(milliseconds: number): string {
  if (milliseconds < 1000) return `${milliseconds} ms`;
  return `${(milliseconds / 1000).toFixed(1)} s`;
}

function folderName(folders: QueryFolder[], folderId: string | null): string {
  return folders.find((folder) => folder.id === folderId)?.name ?? "Unfiled";
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}
