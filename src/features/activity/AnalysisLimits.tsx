import { useEffect, useState } from "react";
import {
  getAgentAnalysisLimits,
  setAgentAnalysisLimits,
  type AgentAnalysisLimits,
} from "../../lib/commands";

const MIB = 1024 * 1024;

type Preset = "conservative" | "balanced" | "large" | "custom";

const presets: Record<Exclude<Preset, "custom">, AgentAnalysisLimits> = {
  conservative: {
    browseRowCap: 1_000,
    maximumResultBytes: 16 * MIB,
    retainedResultLimit: 4,
    outstandingQueryLimit: 2,
    profileCacheBytes: 64 * MIB,
    globalCacheBytes: 256 * MIB,
    queueDeadlineSeconds: 60,
    executionDeadlineSeconds: 60,
  },
  balanced: {
    browseRowCap: 5_000,
    maximumResultBytes: 32 * MIB,
    retainedResultLimit: 8,
    outstandingQueryLimit: 4,
    profileCacheBytes: 128 * MIB,
    globalCacheBytes: 512 * MIB,
    queueDeadlineSeconds: 60,
    executionDeadlineSeconds: 60,
  },
  large: {
    browseRowCap: 25_000,
    maximumResultBytes: 64 * MIB,
    retainedResultLimit: 8,
    outstandingQueryLimit: 4,
    profileCacheBytes: 512 * MIB,
    globalCacheBytes: 1024 * MIB,
    queueDeadlineSeconds: 60,
    executionDeadlineSeconds: 300,
  },
};

function presetFor(limits: AgentAnalysisLimits): Preset {
  return (
    (Object.entries(presets).find(
      ([, value]) => JSON.stringify(value) === JSON.stringify(limits),
    )?.[0] as Preset) ?? "custom"
  );
}

export function AnalysisLimits() {
  const [limits, setLimits] = useState<AgentAnalysisLimits | null>(null);
  const [preset, setPreset] = useState<Preset>("balanced");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    getAgentAnalysisLimits()
      .then((value) => {
        setLimits(value);
        setPreset(presetFor(value));
      })
      .catch((error) => setMessage(`Analysis limits unavailable: ${String(error)}`));
  }, []);

  async function save() {
    if (!limits || busy) return;
    setBusy(true);
    setMessage(null);
    try {
      const saved = await setAgentAnalysisLimits(limits);
      setLimits(saved);
      setPreset(presetFor(saved));
      setMessage("Saved. New limits apply only to newly admitted agent queries.");
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  function choose(next: Preset) {
    setPreset(next);
    if (next !== "custom") setLimits(presets[next]);
  }

  function numberField(
    label: string,
    key: keyof AgentAnalysisLimits,
    minimum: number,
    maximum: number,
    step: number,
    divisor = 1,
  ) {
    if (!limits) return null;
    return (
      <label>
        <span>{label}</span>
        <input
          disabled={preset !== "custom"}
          max={maximum / divisor}
          min={minimum / divisor}
          onChange={(event) =>
            setLimits({ ...limits, [key]: Number(event.target.value) * divisor })
          }
          step={step / divisor}
          type="number"
          value={Number(limits[key]) / divisor}
        />
      </label>
    );
  }

  return (
    <section aria-label="Agent analysis limits" className="analysis-limits">
      <div>
        <strong>Agent analysis limits</strong>
        <span>Desktop-owned. Changes affect new work only.</span>
      </div>
      <div aria-label="Analysis limit preset" className="analysis-preset" role="group">
        {(["conservative", "balanced", "large", "custom"] as const).map((value) => (
          <button
            aria-pressed={preset === value}
            key={value}
            onClick={() => choose(value)}
            type="button"
          >
            {value[0].toUpperCase() + value.slice(1)}
          </button>
        ))}
      </div>
      {limits && (
        <div className="analysis-limit-fields">
          {numberField("Browse rows", "browseRowCap", 100, 50_000, 100)}
          {numberField("Result MiB", "maximumResultBytes", 8 * MIB, 128 * MIB, MIB, MIB)}
          {numberField("Retained results", "retainedResultLimit", 1, 16, 1)}
          {numberField("Outstanding queries", "outstandingQueryLimit", 1, 8, 1)}
          {numberField("Profile cache MiB", "profileCacheBytes", 32 * MIB, 512 * MIB, MIB, MIB)}
          {numberField("Global cache MiB", "globalCacheBytes", 128 * MIB, 1024 * MIB, MIB, MIB)}
          {numberField("Queue deadline seconds", "queueDeadlineSeconds", 10, 60, 10)}
          {numberField("Execution deadline seconds", "executionDeadlineSeconds", 10, 300, 10)}
        </div>
      )}
      <div className="analysis-limit-actions">
        <span role="status">{message}</span>
        <button
          className="text-button"
          disabled={!limits || busy}
          onClick={() => void save()}
          type="button"
        >
          {busy ? "Saving" : "Save limits"}
        </button>
      </div>
    </section>
  );
}
