import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelQuery,
  executeQuery,
  forgetTabExecution,
  getQueryStatus,
  getTabExecution,
  releaseResult,
  type ExecutionError,
  type ExecutionState,
} from "../../lib/commands";

const POLL_INTERVAL_MS = 250;

export interface TabExecution {
  executionId: string;
  sql: string;
  state: ExecutionState;
  durationMs: number;
  rowsProduced: number | null;
  rowsAffected: number | null;
  error: ExecutionError | null;
  resultId: string | null;
  rowTotal: number | null;
}

function isTerminal(state: ExecutionState): boolean {
  return state === "succeeded" || state === "failed" || state === "cancelled";
}

/**
 * Tracks one asynchronous execution per editor tab. The desktop coordinator
 * owns the lifecycle; this hook observes it through short status polls and
 * keeps only lightweight state in the WebView.
 */
export function useQueryExecution(
  projectId: string,
  onSucceeded?: () => void | Promise<void>,
): {
  executions: Record<string, TabExecution>;
  restoring: Record<string, boolean>;
  starting: Record<string, boolean>;
  run: (tabId: string, sql: string) => Promise<void>;
  restore: (tabId: string, sql: string) => Promise<void>;
  cancel: (tabId: string) => Promise<void>;
  forget: (tabId: string) => void;
} {
  const [executions, setExecutions] = useState<Record<string, TabExecution>>({});
  const [restoring, setRestoring] = useState<Record<string, boolean>>({});
  const [starting, setStarting] = useState<Record<string, boolean>>({});
  const timers = useRef(new Map<string, number>());
  const restoredTabs = useRef(new Set<string>());
  const stopped = useRef(false);
  const onSucceededRef = useRef(onSucceeded);
  const executionsRef = useRef(executions);

  useEffect(() => {
    onSucceededRef.current = onSucceeded;
  }, [onSucceeded]);

  useEffect(() => {
    executionsRef.current = executions;
  }, [executions]);

  useEffect(() => {
    const activeTimers = timers.current;
    stopped.current = false;
    return () => {
      stopped.current = true;
      for (const timer of activeTimers.values()) {
        window.clearTimeout(timer);
      }
      activeTimers.clear();
    };
  }, []);

  const patch = useCallback((tabId: string, next: TabExecution) => {
    setExecutions((current) => {
      const updated = { ...current, [tabId]: next };
      executionsRef.current = updated;
      return updated;
    });
  }, []);

  const poll = useCallback(
    (tabId: string, executionId: string) => {
      const tick = async () => {
        if (stopped.current) return;
        try {
          const status = await getQueryStatus(executionId);
          if (stopped.current) return;
          if (status) {
            patch(tabId, {
              executionId,
              sql: status.sql || executionsRef.current[tabId]?.sql || "",
              state: status.state,
              durationMs: status.durationMs,
              rowsProduced: status.rowsProduced,
              rowsAffected: status.rowsAffected,
              error: status.error,
              resultId: status.resultId,
              rowTotal: status.rowTotal,
            });
            if (!isTerminal(status.state)) {
              const timer = window.setTimeout(tick, POLL_INTERVAL_MS);
              timers.current.set(tabId, timer);
            } else {
              timers.current.delete(tabId);
              if (status.state === "succeeded") {
                // Refresh catalog/source metadata exactly once per successful
                // terminal observation. Result rendering does not wait for it.
                Promise.resolve(onSucceededRef.current?.()).catch(() => undefined);
              }
            }
            return;
          }
        } catch {
          // Transient poll failure: keep trying until unmount.
        }
        const timer = window.setTimeout(tick, POLL_INTERVAL_MS);
        timers.current.set(tabId, timer);
      };
      const timer = window.setTimeout(tick, POLL_INTERVAL_MS);
      timers.current.set(tabId, timer);
    },
    [patch],
  );

  const run = useCallback(
    async (tabId: string, sql: string) => {
      const previous = executions[tabId];
      if (previous && (previous.state === "queued" || previous.state === "running")) {
        return;
      }
      // Release the superseded result so its page artifacts are reclaimed
      // before the new execution publishes a fresh one.
      if (previous?.resultId) {
        await releaseResult(previous.resultId).catch(() => undefined);
      }
      setStarting((current) => ({ ...current, [tabId]: true }));
      try {
        const view = await executeQuery(projectId, tabId, sql);
        if (stopped.current) return;
        patch(tabId, {
          executionId: view.executionId,
          sql: view.sql || sql,
          state: view.state,
          durationMs: view.durationMs,
          rowsProduced: view.rowsProduced,
          rowsAffected: view.rowsAffected,
          error: view.error,
          resultId: view.resultId,
          rowTotal: view.rowTotal,
        });
        poll(tabId, view.executionId);
      } finally {
        if (!stopped.current) {
          setStarting((current) => ({ ...current, [tabId]: false }));
        }
      }
    },
    [executions, patch, poll, projectId],
  );

  const restore = useCallback(
    async (tabId: string, sql: string) => {
      const restoreKey = `${projectId}:${tabId}`;
      if (!projectId || restoredTabs.current.has(restoreKey)) return;
      restoredTabs.current.add(restoreKey);
      setRestoring((current) => ({ ...current, [tabId]: true }));
      try {
        const view = await getTabExecution(projectId, tabId);
        if (stopped.current || !view || executionsRef.current[tabId]) return;
        patch(tabId, {
          executionId: view.executionId,
          sql: view.sql || sql,
          state: view.state,
          durationMs: view.durationMs,
          rowsProduced: view.rowsProduced,
          rowsAffected: view.rowsAffected,
          error: view.error,
          resultId: view.resultId,
          rowTotal: view.rowTotal,
        });
        if (!isTerminal(view.state)) poll(tabId, view.executionId);
      } finally {
        if (!stopped.current) {
          setRestoring((current) => ({ ...current, [tabId]: false }));
        }
      }
    },
    [patch, poll, projectId],
  );

  const cancel = useCallback(
    async (tabId: string) => {
      const current = executions[tabId];
      if (!current || isTerminal(current.state)) return;
      const view = await cancelQuery(current.executionId);
      if (stopped.current) return;
      patch(tabId, {
        ...current,
        state: view.state,
        durationMs: view.durationMs,
      });
    },
    [executions, patch],
  );

  const forget = useCallback((tabId: string) => {
    const timer = timers.current.get(tabId);
    if (timer !== undefined) {
      window.clearTimeout(timer);
      timers.current.delete(tabId);
    }
    setExecutions((current) => {
      if (!(tabId in current)) return current;
      const next = { ...current };
      delete next[tabId];
      return next;
    });
    Promise.resolve(forgetTabExecution(tabId)).catch(() => undefined);
  }, []);

  return { executions, restoring, starting, run, restore, cancel, forget };
}
