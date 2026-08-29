import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

function BrokenWorkbench(): never {
  throw new Error("render failed");
}

describe("ErrorBoundary", () => {
  it("shows a recoverable startup error instead of a blank window", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);

    render(
      <ErrorBoundary>
        <BrokenWorkbench />
      </ErrorBoundary>,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("Tarik could not load the workbench");
    expect(screen.getByText("npm run tauri:dev:clean")).toBeInTheDocument();
    consoleError.mockRestore();
  });
});
