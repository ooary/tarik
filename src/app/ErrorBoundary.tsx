import { Component, type ErrorInfo, type ReactNode } from "react";

interface ErrorBoundaryState {
  error: Error | null;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Tarik interface failed to render", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <main className="startup-error" role="alert">
          <div>
            <strong>Tarik could not load the workbench</strong>
            <p>Close any older Tarik development window, then run the clean restart command.</p>
            <code>npm run tauri:dev:clean</code>
            <details>
              <summary>Error details</summary>
              <pre>{this.state.error.message}</pre>
            </details>
          </div>
        </main>
      );
    }

    return this.props.children;
  }
}
