import { Component, type ErrorInfo, type ReactNode } from "react";
import { reportFrontendIncident, type SupportIncident } from "../lib/commands";
import { SupportIncidentNotice } from "./SupportIncidentNotice";

interface ErrorBoundaryState {
  error: Error | null;
  incident: SupportIncident | null;
  reporting: boolean;
}

function newIncidentId(): string {
  return (
    globalThis.crypto?.randomUUID?.() ??
    `${Date.now().toString(16)}-${Math.random().toString(16).slice(2)}`
  );
}

function fallbackIncident(incidentId: string): SupportIncident {
  return {
    incidentId,
    summary: "The workbench interface stopped rendering.",
    logDirectory: "",
    loggingSucceeded: false,
  };
}

export class ErrorBoundary extends Component<{ children: ReactNode }, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null, incident: null, reporting: false };

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Tarik interface failed to render", error, info.componentStack);
    const incidentId = newIncidentId();
    this.setState({ reporting: true });
    reportFrontendIncident({
      incidentId,
      kind: "frontend.render",
      message: `${error.name}: ${error.message}`,
    })
      .then((incident) => this.setState({ incident, reporting: false }))
      .catch(() => this.setState({ incident: fallbackIncident(incidentId), reporting: false }));
  }

  render() {
    if (this.state.error) {
      return (
        <main className="startup-error">
          {this.state.incident ? (
            <SupportIncidentNotice
              heading="Tarik could not load the workbench"
              incident={this.state.incident}
              onRetry={() => window.location.reload()}
            />
          ) : (
            <div role="alert">
              <strong>Tarik could not load the workbench</strong>
              <p>
                {this.state.reporting
                  ? "Recording a local incident before recovery options appear."
                  : "Recovery information is unavailable."}
              </p>
            </div>
          )}
        </main>
      );
    }

    return this.props.children;
  }
}
