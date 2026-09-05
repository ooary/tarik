import { GaugeIcon } from "@phosphor-icons/react";
import { useEffect, useMemo, useState } from "react";
import { Dialog, Field } from "../../components/ui";
import {
  getEngineResources,
  setEngineResources,
  type EngineResourcePreset,
  type EngineResourceSettings,
  type EngineResourceStatus,
} from "../../lib/commands";
import "./engine-resources.css";

interface EngineResourcesDialogProps {
  statusKey: string;
}

type MemoryUnit = "MiB" | "GiB";

interface ResourceDraft {
  preset: EngineResourcePreset;
  memoryAmount: string;
  memoryUnit: MemoryUnit;
  threads: string;
}

const presets: Array<{
  id: Exclude<EngineResourcePreset, "custom">;
  name: string;
  detail: string;
  memoryLimitMib: number;
  threads: number;
}> = [
  {
    id: "low_memory",
    name: "Low memory",
    detail: "512 MiB · 1 thread",
    memoryLimitMib: 512,
    threads: 1,
  },
  {
    id: "balanced",
    name: "Balanced",
    detail: "2 GiB · 2 threads",
    memoryLimitMib: 2_048,
    threads: 2,
  },
  { id: "fast", name: "Fast", detail: "8 GiB · 4 threads", memoryLimitMib: 8_192, threads: 4 },
];

export function EngineResourcesDialog({ statusKey }: EngineResourcesDialogProps) {
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<EngineResourceStatus | null>(null);
  const [draft, setDraft] = useState<ResourceDraft>(() => draftFromSettings(defaultSettings()));
  const [loading, setLoading] = useState(true);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    getEngineResources()
      .then((next) => {
        if (!active) return;
        setStatus(next);
        setDraft(draftFromSettings(next.requested));
        setError(null);
      })
      .catch((cause) => {
        if (active) setError(String(cause));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [statusKey]);

  const parsed = useMemo(() => parseDraft(draft, status), [draft, status]);
  const warning = useMemo(
    () => resourceWarning(parsed.settings, status),
    [parsed.settings, status],
  );

  function choosePreset(preset: EngineResourcePreset) {
    const selected = presets.find((item) => item.id === preset);
    if (selected) {
      setDraft(
        draftFromSettings({
          preset: selected.id,
          memoryLimitMib: selected.memoryLimitMib,
          threads: selected.threads,
        }),
      );
    } else {
      setDraft((current) => ({ ...current, preset: "custom" }));
    }
    setError(null);
  }

  async function apply() {
    if (!parsed.settings || applying) return;
    setApplying(true);
    setError(null);
    try {
      const next = await setEngineResources(parsed.settings);
      setStatus(next);
      setDraft(draftFromSettings(next.requested));
      setOpen(false);
    } catch (cause) {
      setError(resourceErrorMessage(String(cause)));
    } finally {
      setApplying(false);
    }
  }

  return (
    <>
      <button
        aria-label={resourceStatusLabel(status, loading, error)}
        className="status-resource-trigger"
        onClick={() => setOpen(true)}
        type="button"
      >
        <GaugeIcon aria-hidden="true" size={12} />
        {resourceStatusLabel(status, loading, error)}
      </button>
      <Dialog
        busy={applying}
        description="Choose DuckDB working memory and parallelism for every local project."
        onOpenChange={(nextOpen) => {
          setOpen(nextOpen);
          if (nextOpen && status) setDraft(draftFromSettings(status.requested));
          if (!nextOpen) setError(null);
        }}
        open={open}
        title="DuckDB resources"
      >
        <form
          className="engine-resources-form"
          onSubmit={(event) => {
            event.preventDefault();
            void apply();
          }}
        >
          <fieldset className="resource-presets" disabled={applying}>
            <legend>Resource profile</legend>
            {presets.map((preset) => (
              <label key={preset.id}>
                <input
                  checked={draft.preset === preset.id}
                  name="resource-preset"
                  onChange={() => choosePreset(preset.id)}
                  type="radio"
                />
                <span>
                  <strong>{preset.name}</strong>
                  <small>{preset.detail}</small>
                </span>
                {preset.id === "balanced" && <em>Recommended</em>}
              </label>
            ))}
            <label>
              <input
                checked={draft.preset === "custom"}
                name="resource-preset"
                onChange={() => choosePreset("custom")}
                type="radio"
              />
              <span>
                <strong>Custom</strong>
                <small>Set memory and threads independently</small>
              </span>
            </label>
          </fieldset>

          <div className="resource-custom-grid" aria-disabled={draft.preset !== "custom"}>
            <Field
              disabled={draft.preset !== "custom" || applying}
              error={parsed.memoryError}
              label="DuckDB memory"
              min="1"
              onChange={(event) =>
                setDraft((current) => ({ ...current, memoryAmount: event.target.value }))
              }
              step="1"
              type="number"
              value={draft.memoryAmount}
            />
            <label className="resource-unit-field">
              <span>Unit</span>
              <select
                disabled={draft.preset !== "custom" || applying}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    memoryUnit: event.target.value as MemoryUnit,
                  }))
                }
                value={draft.memoryUnit}
              >
                <option value="MiB">MiB</option>
                <option value="GiB">GiB</option>
              </select>
            </label>
            <Field
              disabled={draft.preset !== "custom" || applying}
              error={parsed.threadsError}
              label="Worker threads"
              min="1"
              onChange={(event) =>
                setDraft((current) => ({ ...current, threads: event.target.value }))
              }
              step="1"
              type="number"
              value={draft.threads}
            />
          </div>

          <div className="resource-current" aria-live="polite">
            <span>Current</span>
            <strong>{currentResourceLabel(status)}</strong>
            <small>
              {status?.effective
                ? "Verified by DuckDB for the active project."
                : "Saved request applies when a project session opens."}
            </small>
          </div>

          <p className="resource-limit-note">
            Limits DuckDB working memory. Total Tarik process memory can be higher.
          </p>
          {warning && <p className="resource-warning">{warning}</p>}
          {error && (
            <div className="ui-inline-error" role="alert">
              <strong>Resource settings were not applied</strong>
              <span>{error}</span>
            </div>
          )}
          <div className="ui-dialog-actions">
            <button
              className="toolbar-button"
              disabled={applying}
              onClick={() => setOpen(false)}
              type="button"
            >
              Cancel
            </button>
            <button className="run-button" disabled={!parsed.settings || applying} type="submit">
              {applying ? "Applying…" : "Apply settings"}
            </button>
          </div>
        </form>
      </Dialog>
    </>
  );
}

function defaultSettings(): EngineResourceSettings {
  return { preset: "balanced", memoryLimitMib: 2_048, threads: 2 };
}

function draftFromSettings(settings: EngineResourceSettings): ResourceDraft {
  const useGiB = settings.memoryLimitMib % 1024 === 0;
  return {
    preset: settings.preset,
    memoryAmount: String(useGiB ? settings.memoryLimitMib / 1024 : settings.memoryLimitMib),
    memoryUnit: useGiB ? "GiB" : "MiB",
    threads: String(settings.threads),
  };
}

function parseDraft(
  draft: ResourceDraft,
  status: EngineResourceStatus | null,
): { settings: EngineResourceSettings | null; memoryError?: string; threadsError?: string } {
  const memoryAmount = Number(draft.memoryAmount);
  const threads = Number(draft.threads);
  const memoryLimitMib = memoryAmount * (draft.memoryUnit === "GiB" ? 1024 : 1);
  const minimumMemory = status?.minimumMemoryMib ?? 128;
  const maximumMemory = status?.maximumMemoryMib ?? 262_144;
  const minimumThreads = status?.minimumThreads ?? 1;
  const maximumThreads = status?.maximumThreads ?? 256;
  const memoryError =
    !Number.isSafeInteger(memoryLimitMib) ||
    memoryLimitMib < minimumMemory ||
    memoryLimitMib > maximumMemory
      ? `Use ${minimumMemory.toLocaleString()}–${maximumMemory.toLocaleString()} MiB.`
      : undefined;
  const threadsError =
    !Number.isSafeInteger(threads) || threads < minimumThreads || threads > maximumThreads
      ? `Use ${minimumThreads}–${maximumThreads} threads.`
      : undefined;
  return {
    settings:
      memoryError || threadsError ? null : { preset: draft.preset, memoryLimitMib, threads },
    memoryError,
    threadsError,
  };
}

function resourceWarning(
  settings: EngineResourceSettings | null,
  status: EngineResourceStatus | null,
): string | null {
  if (!settings || !status) return null;
  const warnings = [];
  if (status.physicalMemoryMib && settings.memoryLimitMib > status.physicalMemoryMib) {
    warnings.push("The requested DuckDB memory exceeds detected physical memory.");
  }
  if (status.logicalCpuCount && settings.threads > status.logicalCpuCount) {
    warnings.push("The requested threads exceed detected logical CPUs.");
  }
  return warnings.join(" ") || null;
}

function resourceStatusLabel(
  status: EngineResourceStatus | null,
  loading: boolean,
  error: string | null,
): string {
  if (loading) return "DuckDB resources: Checking";
  if (!status || error) return "DuckDB resources: Unavailable";
  if (!status.effective)
    return `DuckDB resources: ${presetName(status.requested.preset)} · Pending`;
  return `DuckDB resources: ${presetName(status.effective.preset)} · ${formatMemory(status.effective.memoryLimitMib)} · ${formatThreads(status.effective.threads)}`;
}

function currentResourceLabel(status: EngineResourceStatus | null): string {
  if (!status) return "Unavailable";
  const resources = status.effective ?? status.requested;
  return `${presetName(resources.preset)} · ${formatMemory(resources.memoryLimitMib)} · ${formatThreads(resources.threads)}`;
}

function presetName(preset: EngineResourcePreset): string {
  if (preset === "low_memory") return "Low memory";
  return preset[0].toUpperCase() + preset.slice(1);
}

function formatMemory(mib: number): string {
  return mib % 1024 === 0 ? `${mib / 1024} GiB` : `${mib.toLocaleString()} MiB`;
}

function formatThreads(threads: number): string {
  return `${threads} ${threads === 1 ? "thread" : "threads"}`;
}

function resourceErrorMessage(error: string): string {
  if (error.includes("resources.busy")) {
    return "DuckDB is working. Finish or cancel the active query, export, or Actual Flow before applying settings.";
  }
  return error;
}
