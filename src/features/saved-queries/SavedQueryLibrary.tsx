import {
  BookmarksIcon,
  FolderPlusIcon,
  MagnifyingGlassIcon,
  PlusIcon,
  XIcon,
} from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useMemo, useState } from "react";
import { Field } from "../../components/ui";
import {
  createQueryFolder,
  createSavedQuery,
  deleteQueryFolder,
  deleteSavedQuery,
  listQueryFolders,
  listSavedQueries,
  renameQueryFolder,
  updateSavedQuery,
  type QueryFolder,
  type SavedQuery,
  type SavedQueryDraft,
} from "../../lib/commands";

interface SavedQueryLibraryProps {
  projectId: string;
  activeSql: string;
  activeTitle: string;
  onOpenSql: (sql: string, title: string) => void;
}

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
  const [queries, setQueries] = useState<SavedQuery[]>([]);
  const [folders, setFolders] = useState<QueryFolder[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [form, setForm] = useState<FormState | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selected = queries.find((query) => query.id === selectedId) ?? null;

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
    if (!open) return;
    const timeout = window.setTimeout(() => void refresh(search), search ? 180 : 0);
    return () => window.clearTimeout(timeout);
    // refresh intentionally follows current project/open/search only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, projectId, search]);

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
          <div className="query-library-toolbar">
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
          {error && (
            <div className="ui-inline-error" role="alert">
              <strong>Query library error</strong>
              <span>{error}</span>
            </div>
          )}
          <div className="query-library-body">
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
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function folderName(folders: QueryFolder[], folderId: string | null): string {
  return folders.find((folder) => folder.id === folderId)?.name ?? "Unfiled";
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}
