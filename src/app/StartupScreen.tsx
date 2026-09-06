import tarikLogo from "../assets/tarik-logo.png";

export type StartupPhase =
  "preparing" | "storage" | "preferences" | "projects" | "workspace" | "ready";

const PHASES: StartupPhase[] = [
  "preparing",
  "storage",
  "preferences",
  "projects",
  "workspace",
  "ready",
];

const PHASE_LABELS: Record<StartupPhase, string> = {
  preparing: "Preparing Tarik",
  storage: "Checking local storage",
  preferences: "Loading preferences",
  projects: "Restoring projects",
  workspace: "Preparing workspace",
  ready: "Ready",
};

export function StartupScreen({ phase }: { phase: StartupPhase }) {
  const step = PHASES.indexOf(phase);
  const progress = Math.max(8, Math.round(((step + 1) / PHASES.length) * 100));

  return (
    <main className="startup-screen" role="status" aria-live="polite">
      <div className="startup-screen-panel">
        <div className="startup-screen-brand">
          <img alt="" aria-hidden="true" src={tarikLogo} />
          <span>Tarik</span>
        </div>
        <div className="startup-screen-copy">
          <strong>{PHASE_LABELS[phase]}</strong>
          <span>Opening your local SQL workspace.</span>
        </div>
        <div
          aria-label={`Startup progress: ${progress}%`}
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={progress}
          className="startup-progress"
          role="progressbar"
        >
          <span style={{ width: `${progress}%` }} />
        </div>
        <small>
          {phase === "projects" ? "Large local projects can take a moment." : "Local only"}
        </small>
      </div>
    </main>
  );
}
