import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { getRuntimeInfo } from "./lib/commands";

vi.mock("./lib/commands", () => ({
  getRuntimeInfo: vi.fn(),
}));

const runtimeInfoMock = vi.mocked(getRuntimeInfo);

describe("Tarik workbench shell", () => {
  beforeEach(() => {
    runtimeInfoMock.mockReset();
    runtimeInfoMock.mockResolvedValue({
      appName: "Tarik",
      appVersion: "0.1.0",
      rustTarget: "linux",
    });
  });

  it("renders the workbench regions and runtime status", async () => {
    render(<App />);

    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Source explorer" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "SQL workspace" })).toBeInTheDocument();
    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
    expect(await screen.findByText("Connected to local DuckDB")).toBeInTheDocument();
  });

  it("collapses and expands the source explorer", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Collapse source explorer" }));
    expect(screen.getByRole("button", { name: "Expand source explorer" })).toBeInTheDocument();
  });

  it("switches result surfaces with accessible tabs", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("tab", { name: "Flow" }));
    expect(screen.getByText("Query flow is ready")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "Profile" }));
    expect(screen.getByText("Profile is ready after execution")).toBeInTheDocument();
  });

  it("collapses and expands the bottom panel", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Collapse result panel" }));
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Expand result panel" })).toBeInTheDocument();
  });

  it("shows a browser-safe connection state when Tauri is unavailable", async () => {
    runtimeInfoMock.mockRejectedValue(new Error("Tauri unavailable"));

    render(<App />);

    expect(await screen.findByText("Connecting to local DuckDB")).toBeInTheDocument();
  });
});
