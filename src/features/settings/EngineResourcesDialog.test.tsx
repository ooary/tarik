import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  getEngineResources,
  setEngineResources,
  type EngineResourceStatus,
} from "../../lib/commands";
import { EngineResourcesDialog } from "./EngineResourcesDialog";

vi.mock("../../lib/commands", () => ({
  getEngineResources: vi.fn(),
  setEngineResources: vi.fn(),
}));

const pending: EngineResourceStatus = {
  requested: { preset: "balanced", memoryLimitMib: 2_048, threads: 2 },
  effective: null,
  state: "pending",
  logicalCpuCount: 4,
  physicalMemoryMib: 8_192,
  minimumMemoryMib: 128,
  maximumMemoryMib: 262_144,
  minimumThreads: 1,
  maximumThreads: 256,
};

const effective: EngineResourceStatus = {
  ...pending,
  state: "effective",
  effective: {
    preset: "balanced",
    memoryLimitMib: 2_048,
    memoryLimitDisplay: "2.0 GiB",
    threads: 2,
  },
};

describe("EngineResourcesDialog", () => {
  beforeEach(() => {
    vi.mocked(getEngineResources).mockResolvedValue(effective);
    vi.mocked(setEngineResources).mockResolvedValue(effective);
  });

  it("shows only verified effective values in the footer", async () => {
    render(<EngineResourcesDialog statusKey="connected:p1" />);

    expect(
      await screen.findByRole("button", { name: /Balanced · 2 GiB · 2 threads/ }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /DuckDB resources:/ }));
    expect(
      await screen.findByText("Verified by DuckDB for the active project."),
    ).toBeInTheDocument();
    expect(screen.getByText(/Total Tarik process memory can be higher/)).toBeInTheDocument();
  });

  it("shows pending rather than fabricating effective values without a session", async () => {
    vi.mocked(getEngineResources).mockResolvedValue(pending);
    render(<EngineResourcesDialog statusKey="standby:none" />);

    expect(await screen.findByRole("button", { name: /Balanced · Pending/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /DuckDB resources:/ }));
    expect(
      await screen.findByText("Saved request applies when a project session opens."),
    ).toBeInTheDocument();
  });

  it("applies a preset through the typed command", async () => {
    vi.mocked(setEngineResources).mockResolvedValue({
      ...effective,
      requested: { preset: "low_memory", memoryLimitMib: 512, threads: 1 },
      effective: {
        preset: "low_memory",
        memoryLimitMib: 512,
        memoryLimitDisplay: "512.0 MiB",
        threads: 1,
      },
    });
    render(<EngineResourcesDialog statusKey="connected:p1" />);
    fireEvent.click(await screen.findByRole("button", { name: /DuckDB resources:/ }));
    fireEvent.click(screen.getByRole("radio", { name: /Low memory/ }));
    fireEvent.click(screen.getByRole("button", { name: "Apply settings" }));

    await waitFor(() =>
      expect(setEngineResources).toHaveBeenCalledWith({
        preset: "low_memory",
        memoryLimitMib: 512,
        threads: 1,
      }),
    );
    expect(
      await screen.findByRole("button", { name: /Low memory · 512 MiB · 1 thread/ }),
    ).toBeInTheDocument();
  });

  it("validates custom values and warns above detected hardware", async () => {
    render(<EngineResourcesDialog statusKey="connected:p1" />);
    fireEvent.click(await screen.findByRole("button", { name: /DuckDB resources:/ }));
    fireEvent.click(screen.getByRole("radio", { name: /Custom/ }));
    fireEvent.change(screen.getByRole("spinbutton", { name: "DuckDB memory" }), {
      target: { value: "16" },
    });
    fireEvent.change(screen.getByLabelText("Unit"), { target: { value: "GiB" } });
    fireEvent.change(screen.getByRole("spinbutton", { name: "Worker threads" }), {
      target: { value: "8" },
    });

    expect(screen.getByText(/exceeds detected physical memory/)).toBeInTheDocument();
    expect(screen.getByText(/exceed detected logical CPUs/)).toBeInTheDocument();
    fireEvent.change(screen.getByRole("spinbutton", { name: "DuckDB memory" }), {
      target: { value: "0" },
    });
    expect(screen.getByText(/Use 128–262,144 MiB/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply settings" })).toBeDisabled();
  });

  it("keeps the modal open and explains active-work refusal", async () => {
    vi.mocked(setEngineResources).mockRejectedValue(new Error("resources.busy"));
    render(<EngineResourcesDialog statusKey="connected:p1" />);
    fireEvent.click(await screen.findByRole("button", { name: /DuckDB resources:/ }));
    fireEvent.click(screen.getByRole("button", { name: "Apply settings" }));

    expect(await screen.findByText(/Finish or cancel the active query/)).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "DuckDB resources" })).toBeInTheDocument();
  });
});
