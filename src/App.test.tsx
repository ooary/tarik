import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { getRuntimeInfo } from "./lib/commands";

vi.mock("./lib/commands", () => ({
  getRuntimeInfo: vi.fn(),
}));

const runtimeInfoMock = vi.mocked(getRuntimeInfo);

describe("foundation screen", () => {
  beforeEach(() => {
    runtimeInfoMock.mockReset();
  });

  it("shows the typed Rust command response", async () => {
    runtimeInfoMock.mockResolvedValue({
      appName: "Tarik",
      appVersion: "0.1.0",
      rustTarget: "linux",
    });

    render(<App />);

    expect(
      screen.getByRole("heading", { name: "Desktop foundation is running" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("Tarik 0.1.0")).toBeInTheDocument();
    expect(screen.getByText("Command round-trip verified")).toBeInTheDocument();
  });

  it("shows a browser-safe fallback when Tauri is unavailable", async () => {
    runtimeInfoMock.mockRejectedValue(new Error("Tauri unavailable"));

    render(<App />);

    expect(await screen.findByText("Browser preview mode")).toBeInTheDocument();
    expect(screen.getByText("Foundation preview")).toBeInTheDocument();
  });
});
