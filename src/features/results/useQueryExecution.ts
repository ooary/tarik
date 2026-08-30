import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelQuery,
  executeQuery,
  forgetTabExecution,
  getQueryStatus,
  releaseResult,
  type ExecutionError,
  type ExecutionState,
} from "../../lib/commands";

const POLL_INTERVAL_MS = 250;

export interface TabExecution {
  executionId: string;
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
export function useQueryExecution(projectId: string): {
  executions: Record<string, TabExecution>;
  run: (tabId: string, sql: string) => Promise<void>;
  cancel: (tabId: string) => Promise<void>;
  forget: (tabId: string) => void;
} {
  const [executions, setExecutions] = useState<Record<string, TabExecution>>({});
  const timers = useRef(new Map<string, number>());
  const stopped = useRef(false);

  useEffect(() => {
    stopped.current = false;
    return () => {
      stopped.current = true;
      for (const timer of timers.current.values()) {
        window.clearTimeout(timer);
      }
      timers.current.clear();
    };
  }, []);

  const patch = useCallback((tabId: string, next: TabExecution) => {
    setExecutions((current) => ({ ...current, [tabId]: next }));
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
      const view = await executeQuery(projectId, tabId, sql);
      if (stopped.current) return;
      patch(tabId, {
        executionId: view.executionId,
        state: view.state,
        durationMs: view.durationMs,
        rowsProduced: view.rowsProduced,
        rowsAffected: view.rowsAffected,
        error: view.error,
        resultId: view.resultId,
        rowTotal: view.rowTotal,
      });
      poll(tabId, view.executionId);
    },
    [executions, patch, poll, projectId],
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

  return { executions, run, cancel, forget };
}
