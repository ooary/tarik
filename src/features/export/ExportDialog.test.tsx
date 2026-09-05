import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cancelExport,
  chooseExportDirectory,
  executeExport,
  getExportStatus,
  revealExportPart,
  type ExportView,
} from "../../lib/commands";
import { ExportDialog } from "./ExportDialog";

vi.mock("../../lib/commands", () => ({
  cancelExport: vi.fn(),
  chooseExportDirectory: vi.fn(),
  executeExport: vi.fn(),
  getExportStatus: vi.fn(),
  revealExportPart: vi.fn(),
}));

const queued: ExportView = {
  exportId: "export-12345678",
  projectId: "p1",
  state: "queued",
  durationMs: 0,
  rowsWritten: 0,
  filesWritten: 0,
  bytesWritten: 0,
  currentPart: null,
  completedParts: [],
  error: null,
};

const running: ExportView = {
  ...queued,
  state: "running",
  durationMs: 850,
  rowsWritten: 1200,
  filesWritten: 1,
  bytesWritten: 4096,
  currentPart: 2,
  completedParts: [
    {
      partNumber: 1,
      path: "/exports/orders-part-00001.parquet",
      rows: 1000,
      bytes: 3200,
    },
  ],
};

const succeeded: ExportView = {
  ...running,
  state: "succeeded",
  durationMs: 1250,
  rowsWritten: 1500,
  filesWritten: 2,
  bytesWritten: 5200,
  currentPart: null,
  completedParts: [
    running.completedParts[0],
    {
      partNumber: 2,
      path: "/exports/orders-part-00002.parquet",
      rows: 500,
      bytes: 2000,
    },
  ],
};

describe("ExportDialog", () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
    vi.mocked(chooseExportDirectory).mockResolvedValue("/exports");
    vi.mocked(executeExport).mockResolvedValue(queued);
    vi.mocked(getExportStatus).mockResolvedValue(succeeded);
    vi.mocked(cancelExport).mockResolvedValue({
      ...running,
      state: "cancelled",
      currentPart: null,
    });
    vi.mocked(revealExportPart).mockResolvedValue(undefined);
  });

  it("disables export without project SQL and validates before submission", async () => {
    const { rerender } = render(<ExportDialog projectId="" sql="" suggestedName="Untitled" />);
    expect(screen.getByRole("button", { name: "Export" })).toBeDisabled();

    rerender(<ExportDialog projectId="p1" sql="SELECT 1" suggestedName="Untitled" />);
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    expect(screen.getByRole("dialog", { name: "Export query" })).toBeInTheDocument();
    expect(screen.getByText("SELECT 1")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Start export" }));
    expect(await screen.findByText("Choose an existing output folder.")).toBeInTheDocument();
    expect(executeExport).not.toHaveBeenCalled();
  });

  it("requires confirmation before exporting potentially mutating SQL", async () => {
    render(<ExportDialog projectId="p1" sql="DELETE FROM orders" suggestedName="delete" />);
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose output folder" }));
    await waitFor(() => expect(screen.getByLabelText("Output folder")).toHaveValue("/exports"));
    fireEvent.click(screen.getByRole("button", { name: "Start export" }));
    expect(await screen.findByText(/Export executes this SQL once/)).toBeInTheDocument();
    expect(executeExport).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Start export" }));
    await waitFor(() =>
      expect(executeExport).toHaveBeenCalledWith(
        "p1",
        "DELETE FROM orders",
        expect.objectContaining({ baseName: "delete" }),
      ),
    );
  });

  it("submits canonical Parquet options, polls, and reveals completed output", async () => {
    render(
      <ExportDialog projectId="p1" sql="SELECT * FROM orders" suggestedName="Orders report" />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose output folder" }));
    await waitFor(() => expect(screen.getByLabelText("Output folder")).toHaveValue("/exports"));
    fireEvent.change(screen.getByLabelText("Rows per part"), { target: { value: "1000" } });
    fireEvent.click(screen.getByRole("button", { name: "Start export" }));

    await waitFor(() =>
      expect(executeExport).toHaveBeenCalledWith("p1", "SELECT * FROM orders", {
        format: "parquet",
        outputDirectory: "/exports",
        baseName: "Orders_report",
        rowsPerPart: 1000,
        overwrite: "fail_if_exists",
        csv: null,
        parquet: { compression: "snappy" },
      }),
    );
    expect(await screen.findByText("Waiting to start")).toBeInTheDocument();
    expect(await screen.findByText("Export complete", {}, { timeout: 1500 })).toBeInTheDocument();
    expect(screen.getByText("1.5K")).toBeInTheDocument();
    expect(screen.getByText("500 rows")).toBeInTheDocument();
    expect(screen.getByText("orders-part-00002.parquet", { exact: false })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Reveal output" }));
    expect(revealExportPart).toHaveBeenCalledWith("export-12345678", 1);
  });

  it("sends CSV options and requests active cancellation", async () => {
    vi.mocked(getExportStatus).mockResolvedValue(running);
    render(<ExportDialog projectId="p1" sql="SELECT 1" suggestedName="query" />);
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("radio", { name: /CSV/ }));
    fireEvent.click(screen.getByRole("button", { name: "Choose output folder" }));
    await waitFor(() => expect(screen.getByLabelText("Output folder")).toHaveValue("/exports"));
    fireEvent.change(screen.getByLabelText("Delimiter"), { target: { value: "|" } });
    fireEvent.click(screen.getByLabelText("Include column names in every part"));
    fireEvent.click(screen.getByRole("button", { name: "Start export" }));
    await screen.findByText("Waiting to start");

    await waitFor(() =>
      expect(executeExport).toHaveBeenCalledWith(
        "p1",
        "SELECT 1",
        expect.objectContaining({
          format: "csv",
          csv: { delimiter: "|", includeHeader: false },
          parquet: null,
        }),
      ),
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Cancel export" })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel export" }));
    await waitFor(() => expect(cancelExport).toHaveBeenCalledWith("export-12345678"));
  });

  it("explains zero-row success and valid partial files after failure", async () => {
    vi.mocked(executeExport).mockResolvedValue({
      ...queued,
      state: "succeeded",
    });
    const { unmount } = render(
      <ExportDialog projectId="p1" sql="SELECT 1 WHERE false" suggestedName="empty" />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose output folder" }));
    await waitFor(() => expect(screen.getByLabelText("Output folder")).toHaveValue("/exports"));
    fireEvent.click(screen.getByRole("button", { name: "Start export" }));
    expect(
      await screen.findByText("Query returned no rows; no files were created."),
    ).toBeInTheDocument();
    unmount();

    vi.mocked(executeExport).mockResolvedValue({
      ...running,
      state: "failed",
      currentPart: null,
      error: { code: "export.io", message: "disk full" },
    });
    render(<ExportDialog projectId="p1" sql="SELECT * FROM large" suggestedName="large" />);
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose output folder" }));
    await waitFor(() => expect(screen.getByLabelText("Output folder")).toHaveValue("/exports"));
    fireEvent.click(screen.getByRole("button", { name: "Start export" }));
    expect(await screen.findByText("disk full")).toBeInTheDocument();
    expect(
      screen.getByText("Completed files remain valid. The incomplete current part was removed."),
    ).toBeInTheDocument();
  });

  it("keeps the submitted SQL snapshot and discloses bounded part summaries", async () => {
    const manyParts = Array.from({ length: 100 }, (_, index) => ({
      partNumber: index + 21,
      path: `/exports/orders-part-${String(index + 21).padStart(5, "0")}.parquet`,
      rows: 1000,
      bytes: 2048,
    }));
    vi.mocked(executeExport).mockResolvedValue({
      ...succeeded,
      filesWritten: 120,
      completedParts: manyParts,
    });
    const { rerender } = render(
      <ExportDialog projectId="p1" sql="SELECT 1 AS original" suggestedName="query" />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose output folder" }));
    await waitFor(() => expect(screen.getByLabelText("Output folder")).toHaveValue("/exports"));
    fireEvent.click(screen.getByRole("button", { name: "Start export" }));
    expect(await screen.findByText("Export complete")).toBeInTheDocument();

    rerender(<ExportDialog projectId="p1" sql="SELECT 2 AS edited" suggestedName="query" />);
    const snapshot = screen.getByRole("region", { name: "Submitted SQL snapshot" });
    expect(snapshot).toHaveTextContent("SELECT 1 AS original");
    expect(snapshot).not.toHaveTextContent("SELECT 2 AS edited");
    expect(screen.getByText("Showing the latest 100 of 120 completed files.")).toBeInTheDocument();
  });

  it("keeps polling after the dialog closes", async () => {
    vi.useFakeTimers();
    vi.mocked(getExportStatus).mockResolvedValueOnce(running).mockResolvedValueOnce(succeeded);
    render(<ExportDialog projectId="p1" sql="SELECT 1" suggestedName="query" />);
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose output folder" }));
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: "Start export" }));
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: "Close export" }));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(400);
    });
    expect(getExportStatus).toHaveBeenCalled();
    expect(cancelExport).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});
