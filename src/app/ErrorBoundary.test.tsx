import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { reportFrontendIncident, revealLogDirectory } from "../lib/commands";
import { ErrorBoundary } from "./ErrorBoundary";

vi.mock("../lib/commands", () => ({
  reportFrontendIncident: vi.fn(),
  revealLogDirectory: vi.fn(),
}));

function BrokenWorkbench(): never {
  throw new Error("render failed");
}

describe("ErrorBoundary", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(reportFrontendIncident).mockResolvedValue({
      incidentId: "11111111-1111-4111-8111-111111111111",
      summary: "The workbench interface stopped rendering.",
      logDirectory: "/tmp/tarik/logs",
      loggingSucceeded: true,
    });
  });

  it("shows a friendly incident and local support actions instead of a blank window", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);

    render(
      <ErrorBoundary>
        <BrokenWorkbench />
      </ErrorBoundary>,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("Tarik could not load the workbench");
    expect(screen.getByRole("alert")).not.toHaveTextContent("render failed");
    expect(await screen.findByText("11111111-1111-4111-8111-111111111111")).toBeInTheDocument();
    expect(reportFrontendIncident).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "frontend.render", message: "Error: render failed" }),
    );

    screen.getByRole("button", { name: "Reveal logs" }).click();
    expect(revealLogDirectory).toHaveBeenCalledWith();
    consoleError.mockRestore();
  });

  it("keeps recovery instructions when backend incident reporting is unavailable", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.mocked(reportFrontendIncident).mockRejectedValue(new Error("backend unavailable"));

    render(
      <ErrorBoundary>
        <BrokenWorkbench />
      </ErrorBoundary>,
    );

    await waitFor(() =>
      expect(screen.getByText("The workbench interface stopped rendering.")).toBeInTheDocument(),
    );
    expect(screen.getByText(/could not be written to the log file/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy ID" })).toBeInTheDocument();
    consoleError.mockRestore();
  });
});
