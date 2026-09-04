import { useEffect, useRef, useState } from "react";
import { validateQuery, type SqlDiagnostic } from "../../lib/commands";

type SqlValidationState =
  | { status: "idle" | "editing" | "checking"; diagnostics: SqlDiagnostic[]; message: null }
  | { status: "clean"; diagnostics: []; message: null }
  | { status: "problems"; diagnostics: SqlDiagnostic[]; message: null }
  | { status: "unavailable"; diagnostics: []; message: string };

type ValidationInput = {
  projectId: string;
  tabId: string;
  sql: string;
  catalogRevision: unknown;
};

type StoredValidation = ValidationInput & { view: SqlValidationState };

const idleState = (): SqlValidationState => ({ status: "idle", diagnostics: [], message: null });
const editingState = (): SqlValidationState => ({
  status: "editing",
  diagnostics: [],
  message: null,
});

export function useSqlValidation(
  projectId: string,
  tabId: string | undefined,
  sql: string | undefined,
  catalogRevision: unknown,
  delayMs = 650,
): SqlValidationState {
  const [stored, setStored] = useState<StoredValidation | null>(null);
  const revision = useRef(0);
  const snapshot = sql ?? "";
  const active = Boolean(projectId && tabId && snapshot.trim());
  const current = Boolean(
    stored &&
    stored.projectId === projectId &&
    stored.tabId === tabId &&
    stored.sql === snapshot &&
    stored.catalogRevision === catalogRevision,
  );

  useEffect(() => {
    revision.current += 1;
    const currentRevision = revision.current;
    if (!projectId || !tabId || !snapshot.trim()) return;
    const input: ValidationInput = { projectId, tabId, sql: snapshot, catalogRevision };
    const timer = window.setTimeout(() => {
      setStored({
        ...input,
        view: { status: "checking", diagnostics: [], message: null },
      });
      validateQuery(projectId, snapshot, currentRevision)
        .then((validation) => {
          if (revision.current !== currentRevision || validation.revision !== currentRevision) {
            return;
          }
          setStored({
            ...input,
            view:
              validation.diagnostics.length === 0
                ? { status: "clean", diagnostics: [], message: null }
                : { status: "problems", diagnostics: validation.diagnostics, message: null },
          });
        })
        .catch((cause) => {
          if (revision.current !== currentRevision) return;
          setStored({
            ...input,
            view: { status: "unavailable", diagnostics: [], message: String(cause) },
          });
        });
    }, delayMs);
    return () => window.clearTimeout(timer);
  }, [catalogRevision, delayMs, projectId, snapshot, tabId]);

  if (!active) return idleState();
  return current && stored ? stored.view : editingState();
}
