import { useCallback, useState } from "react";
import { explainQueryPlan, type PlanMode, type QueryPlan } from "../../lib/commands";

export type PlanViewState =
  | { status: "empty"; plan: null; error: null; sql: null }
  | { status: "loading"; plan: null; error: null; sql: string }
  | { status: "ready"; plan: QueryPlan; error: null; sql: string }
  | { status: "error"; plan: null; error: string; sql: string };

const emptyState = (): PlanViewState => ({ status: "empty", plan: null, error: null, sql: null });

export function useQueryPlan(projectId: string): {
  states: Record<PlanMode, PlanViewState>;
  runPlan: (sql: string, mode: PlanMode) => Promise<void>;
  clearPlan: () => void;
} {
  const [states, setStates] = useState<Record<PlanMode, PlanViewState>>({
    explain: emptyState(),
    profile: emptyState(),
  });

  const runPlan = useCallback(
    async (sql: string, mode: PlanMode) => {
      if (!projectId || !sql.trim()) return;
      setStates((current) => ({
        ...current,
        [mode]: { status: "loading", plan: null, error: null, sql },
      }));
      try {
        const plan = await explainQueryPlan(projectId, sql, mode);
        setStates((current) => ({
          ...current,
          [mode]: { status: "ready", plan, error: null, sql },
        }));
      } catch (error) {
        setStates((current) => ({
          ...current,
          [mode]: {
            status: "error",
            plan: null,
            error: error instanceof Error ? error.message : String(error),
            sql,
          },
        }));
      }
    },
    [projectId],
  );

  const clearPlan = useCallback(() => {
    setStates({ explain: emptyState(), profile: emptyState() });
  }, []);

  return { states, runPlan, clearPlan };
}
