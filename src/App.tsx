import { useEffect, useState } from "react";
import "./App.css";
import { getRuntimeInfo, type RuntimeInfo } from "./lib/commands";

type RuntimeState =
  | { kind: "loading" }
  | { kind: "ready"; info: RuntimeInfo }
  | { kind: "unavailable" };

function App() {
  const [runtime, setRuntime] = useState<RuntimeState>({ kind: "loading" });

  useEffect(() => {
    let active = true;

    getRuntimeInfo()
      .then((info) => {
        if (active) setRuntime({ kind: "ready", info });
      })
      .catch(() => {
        if (active) setRuntime({ kind: "unavailable" });
      });

    return () => {
      active = false;
    };
  }, []);

  return (
    <main className="foundation-shell">
      <section aria-labelledby="foundation-title" className="foundation-panel">
        <p className="product-name">Tarik</p>
        <h1 id="foundation-title">Desktop foundation is running</h1>
        <p className="foundation-copy">
          The local Tauri and React boundary is ready. The workbench interface starts in EPIC E1 after manual review.
        </p>

        <dl aria-live="polite" className="runtime-details">
          <div>
            <dt>Frontend</dt>
            <dd>React + TypeScript + Vite</dd>
          </div>
          <div>
            <dt>Backend</dt>
            <dd>
              {runtime.kind === "loading" && "Connecting to Rust"}
              {runtime.kind === "unavailable" && "Browser preview mode"}
              {runtime.kind === "ready" && `${runtime.info.appName} ${runtime.info.appVersion}`}
            </dd>
          </div>
          <div>
            <dt>Status</dt>
            <dd>{runtime.kind === "ready" ? "Command round-trip verified" : "Foundation preview"}</dd>
          </div>
        </dl>
      </section>
    </main>
  );
}

export default App;
