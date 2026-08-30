import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  loadQuerySession,
  saveQuerySession,
  type QuerySessionSnapshot,
  type QueryTabSnapshot,
} from "../../lib/commands";

export interface WorkbenchTab {
  id: string;
  title: string;
  sql: string;
  dirty: boolean;
}

const DEFAULT_TITLE = "Untitled";
const SAVE_DELAY_MS = 400;

function newTabId(): string {
  return `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function initialTab(): WorkbenchTab {
  return { id: newTabId(), title: DEFAULT_TITLE, sql: "", dirty: false };
}

function initialTabsState() {
  const tab = initialTab();
  return { tabs: [tab], activeTabId: tab.id };
}

export function sessionIdForProject(projectId: string): string {
  return `project-${projectId}-main`;
}

function tabToSnapshot(tab: WorkbenchTab, position: number, isActive: boolean): QueryTabSnapshot {
  return {
    id: tab.id,
    title: tab.title,
    sqlText: tab.sql,
    position,
    isActive,
  };
}

function snapshotFor(
  projectId: string,
  tabs: WorkbenchTab[],
  activeTabId: string,
): QuerySessionSnapshot {
  return {
    id: sessionIdForProject(projectId),
    projectId,
    tabs: tabs.map((tab, index) => tabToSnapshot(tab, index, tab.id === activeTabId)),
  };
}

export function useQueryTabs(projectId: string) {
  const initial = useMemo(() => initialTabsState(), []);
  const [tabs, setTabs] = useState<WorkbenchTab[]>(initial.tabs);
  const [activeTabId, setActiveTabId] = useState(initial.activeTabId);
  const [restored, setRestored] = useState(projectId.length === 0);
  const [saveError, setSaveError] = useState<string | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const saveInFlight = useRef(false);
  const pendingSave = useRef<{
    snapshot: QuerySessionSnapshot;
    revision: number;
  } | null>(null);
  const drainPromise = useRef<Promise<void>>(Promise.resolve());
  const queuedRevision = useRef(-1);
  const revision = useRef(0);
  const latest = useRef({ tabs: initial.tabs, activeTabId: initial.activeTabId });

  useEffect(() => {
    latest.current = { tabs, activeTabId };
  }, [tabs, activeTabId]);

  useEffect(() => {
    if (!projectId) return;
    let active = true;
    loadQuerySession(sessionIdForProject(projectId))
      .then((snapshot) => {
        if (!active) return;
        if (!snapshot || snapshot.projectId !== projectId || snapshot.tabs.length === 0) {
          setRestored(true);
          return;
        }
        const restoredTabs = snapshot.tabs.map((tab): WorkbenchTab => ({
          id: tab.id,
          title: tab.title,
          sql: tab.sqlText,
          dirty: false,
        }));
        const restoredActive = snapshot.tabs.find((tab) => tab.isActive)?.id ?? restoredTabs[0].id;
        setTabs(restoredTabs);
        setActiveTabId(restoredActive);
        setRestored(true);
      })
      .catch((error) => {
        if (active) {
          setSaveError(`Session restore failed: ${String(error)}`);
          setRestored(true);
        }
      });
    return () => {
      active = false;
    };
  }, [projectId]);

  const markChanged = useCallback(() => {
    revision.current += 1;
  }, []);

  const selectTab = useCallback(
    (id: string) => {
      markChanged();
      setActiveTabId(id);
    },
    [markChanged],
  );

  const addTab = useCallback(
    (sql = "") => {
      const tab: WorkbenchTab = {
        id: newTabId(),
        title: DEFAULT_TITLE,
        sql,
        dirty: sql.length > 0,
      };
      markChanged();
      setTabs((current) => [...current, tab]);
      setActiveTabId(tab.id);
      return tab.id;
    },
    [markChanged],
  );

  const closeTab = useCallback(
    (id: string) => {
      markChanged();
      setTabs((current) => {
        const index = current.findIndex((tab) => tab.id === id);
        if (index === -1) return current;
        const remaining = current.filter((tab) => tab.id !== id);
        if (remaining.length === 0) {
          const fresh = initialTab();
          setActiveTabId(fresh.id);
          return [fresh];
        }
        const fallback = current[index - 1] ?? remaining[0];
        setActiveTabId((active) => (active === id ? fallback.id : active));
        return remaining;
      });
    },
    [markChanged],
  );

  const renameTab = useCallback(
    (id: string, title: string) => {
      const normalized = title.trim();
      if (!normalized) return;
      markChanged();
      setTabs((current) =>
        current.map((tab) => (tab.id === id ? { ...tab, title: normalized, dirty: true } : tab)),
      );
    },
    [markChanged],
  );

  const duplicateTab = useCallback(
    (id: string) => {
      markChanged();
      setTabs((current) => {
        const index = current.findIndex((tab) => tab.id === id);
        if (index < 0) return current;
        const source = current[index];
        const copy: WorkbenchTab = {
          ...source,
          id: newTabId(),
          title: `${source.title} copy`,
          dirty: true,
        };
        const next = [...current];
        next.splice(index + 1, 0, copy);
        setActiveTabId(copy.id);
        return next;
      });
    },
    [markChanged],
  );

  const moveTab = useCallback(
    (id: string, offset: -1 | 1) => {
      markChanged();
      setTabs((current) => {
        const index = current.findIndex((tab) => tab.id === id);
        const destination = index + offset;
        if (index < 0 || destination < 0 || destination >= current.length) return current;
        const next = [...current];
        const [tab] = next.splice(index, 1);
        next.splice(destination, 0, tab);
        return next;
      });
    },
    [markChanged],
  );

  const editSql = useCallback(
    (id: string, sql: string) => {
      markChanged();
      setTabs((current) =>
        current.map((tab) => (tab.id === id ? { ...tab, sql, dirty: true } : tab)),
      );
    },
    [markChanged],
  );

  const enqueueSave = useCallback((snapshot: QuerySessionSnapshot, savedRevision: number) => {
    if (queuedRevision.current === savedRevision) return drainPromise.current;
    queuedRevision.current = savedRevision;
    pendingSave.current = { snapshot, revision: savedRevision };
    if (saveInFlight.current) return drainPromise.current;

    saveInFlight.current = true;
    drainPromise.current = (async () => {
      while (pendingSave.current) {
        const next = pendingSave.current;
        pendingSave.current = null;
        try {
          await saveQuerySession(next.snapshot);
          if (revision.current === next.revision) {
            setTabs((current) => current.map((tab) => ({ ...tab, dirty: false })));
          }
          setSaveError(null);
        } catch (error) {
          if (!pendingSave.current) pendingSave.current = next;
          queuedRevision.current = -1;
          setSaveError(`Draft save failed: ${String(error)}`);
          break;
        }
      }
    })().finally(() => {
      saveInFlight.current = false;
    });
    return drainPromise.current;
  }, []);

  const flush = useCallback(() => {
    if (!projectId || revision.current === 0) return Promise.resolve();
    const current = latest.current;
    return enqueueSave(snapshotFor(projectId, current.tabs, current.activeTabId), revision.current);
  }, [enqueueSave, projectId]);

  useEffect(() => {
    if (!projectId || !restored || revision.current === 0) return;
    if (saveTimer.current) clearTimeout(saveTimer.current);
    const savedRevision = revision.current;
    saveTimer.current = setTimeout(() => {
      enqueueSave(snapshotFor(projectId, tabs, activeTabId), savedRevision);
    }, SAVE_DELAY_MS);
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    };
  }, [tabs, activeTabId, projectId, restored, enqueueSave]);

  useEffect(() => {
    const onPageHide = () => {
      void flush();
    };
    window.addEventListener("pagehide", onPageHide);
    return () => {
      window.removeEventListener("pagehide", onPageHide);
      if (saveTimer.current) clearTimeout(saveTimer.current);
      void flush();
    };
  }, [flush]);

  return {
    tabs,
    activeTabId,
    restored,
    saveError,
    selectTab,
    addTab,
    closeTab,
    renameTab,
    duplicateTab,
    moveTab,
    editSql,
    flush,
  };
}
