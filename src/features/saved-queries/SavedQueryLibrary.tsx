import {
  BookmarksIcon,
  ClockCounterClockwiseIcon,
  FolderPlusIcon,
  MagnifyingGlassIcon,
  PlusIcon,
  XIcon,
} from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useMemo, useState } from "react";
import { Field } from "../../components/ui";
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
    // refresh intentionally follows current project/open/search only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, view, projectId, search]);

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

  const startCreate = () => {
    setError(null);
    setForm({
      ...emptyForm(),
      name: activeTitle === "Untitled" ? "" : activeTitle,
      sqlText: activeSql,
    });
  };

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

  const save = async () => {
    if (!form) return;
    const draft: SavedQueryDraft = {
      projectId,
      folderId: form.folderId || null,
      name: form.name,
      sqlText: form.sqlText,
      tags: form.tagsText
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean),
    };
    if (
      form.id &&
      form.originalSql !== form.sqlText &&
      !window.confirm(
        `Replace the SQL stored in "${form.name.trim() || "this saved query"}"?\n\nThe previous saved SQL will be overwritten. Your editor draft is not changed.`,
      )
    ) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const saved = form.id
        ? await updateSavedQuery(form.id, draft)
        : await createSavedQuery(draft);
      setForm(null);
      await refresh();
      setSelectedId(saved.id);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  };

  const addFolder = async () => {
    const name = window.prompt("Folder name")?.trim();
    if (!name) return;
    try {
      await createQueryFolder(projectId, name);
      await refresh();
    } catch (cause) {
      setError(String(cause));
    }
  };

  const renameFolder = async (folder: QueryFolder) => {
    const name = window.prompt("Rename folder", folder.name)?.trim();
    if (!name || name === folder.name) return;
    try {
      await renameQueryFolder(projectId, folder.id, name);
      await refresh();
    } catch (cause) {
      setError(String(cause));
    }
  };

  const removeFolder = async (folder: QueryFolder) => {
    if (
      !window.confirm(
        `Delete folder "${folder.name}"?\n\nSaved queries in this folder will be kept in Unfiled.`,
      )
    ) {
      return;
    }
    try {
      await deleteQueryFolder(projectId, folder.id);
      await refresh();
    } catch (cause) {
      setError(String(cause));
    }
  };

  const applyRetention = async () => {
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
    if (
      !window.confirm(
        `Apply history retention?\n\nThis permanently removes matching history for this project only. Saved queries and editor drafts are kept.`,
      )
    ) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const summary = await applyQueryHistoryRetention(projectId, {
        maxCount: count,
        maxAgeDays: days,
      });
      setRetentionSummary(
        `Deleted ${summary.deleted.toLocaleString()} entries. ${summary.remaining.toLocaleString()} remain.`,
      );
      setHistoryOffset(0);
      setHistoryRevision((current) => current + 1);
      setRetentionOpen(false);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  };

  const clearHistory = async () => {
    if (
      !window.confirm(
        "Clear all query history for this project?\n\nThis cannot be undone. Saved queries and editor drafts are kept.",
      )
    ) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const summary = await clearQueryHistory(projectId);
      setHistory([]);
      setSelectedHistoryId(null);
      setHistoryNextOffset(null);
      setHistoryOffset(0);
      setHistoryRevision((current) => current + 1);
      setRetentionSummary(`Deleted ${summary.deleted.toLocaleString()} history entries.`);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  };

  const removeQuery = async (query: SavedQuery) => {
    if (!window.confirm(`Delete saved query "${query.name}"?\n\nThis does not affect editor tabs.`))
      return;
    try {
      await deleteSavedQuery(projectId, query.id);
      setForm(null);
      await refresh();
    } catch (cause) {
      setError(String(cause));
    }
  };

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
          <div className="query-library-toolbar history-filter-toolbar" hidden={view !== "history"}>
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
              ) : queries.length === 0 && !search ? (
                <div className="saved-query-empty">
                  <strong>No saved queries yet</strong>
                  <span>Save the active editor SQL to keep it in this project.</span>
                </div>
              ) : (
                <>
                  {renderGroup("Unfiled", "", grouped.get("") ?? [])}
                  {folders.map((folder) =>
                    renderGroup(folder.name, folder.id, grouped.get(folder.id) ?? [], folder),
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
  );
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
