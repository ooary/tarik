import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SourceInspection } from "../../lib/commands";
import { formatCompactCount, ImportDialog } from "./ImportDialog";

const csvInspection: SourceInspection = {
  path: "/data/orders.csv",
  format: "csv",
  suggestedName: "orders",
  fileSizeBytes: 12_400,
  rowCount: 1_000,
  rowCountExact: true,
  columns: [
    { name: "id", dataType: "BIGINT", nullable: true },
    { name: "amount", dataType: "DOUBLE", nullable: true },
  ],
  previewRows: [[1, 12.5]],
  csvOptions: { delimiter: ",", hasHeader: true, nullValue: null, allVarchar: false },
  warnings: [],
};

describe("ImportDialog", () => {
  it("shows CSV parsing, schema overrides, preview, and submits trusted options", () => {
    const submit = vi.fn();
    render(
      <ImportDialog
        busy={false}
        error={null}
        inspection={csvInspection}
        onCancel={vi.fn()}
        onClose={vi.fn()}
        onInspectCsv={vi.fn(async () => undefined)}
        onSubmit={submit}
      />,
    );

    expect(screen.getByRole("region", { name: "CSV parsing" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Inferred schema" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Data preview" })).toHaveTextContent("12.5");
    expect(screen.getByLabelText("Source summary")).toHaveTextContent("1K");
    expect(screen.getByLabelText("Override type for amount")).toHaveRole("combobox");
    expect(screen.getByRole("option", { name: "Keep inferred (DOUBLE)" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Override type for amount"), {
      target: { value: "DECIMAL(18,2)" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Import table" }));

    expect(submit).toHaveBeenCalledWith(
      "import",
      expect.objectContaining({
        tableName: "orders",
        columnOverrides: [{ column: "amount", dataType: "DECIMAL(18,2)" }],
      }),
    );
  });

  it("formats compact counts at readable boundaries", () => {
    expect(formatCompactCount(999)).toBe("999");
    expect(formatCompactCount(1_000)).toBe("1K");
    expect(formatCompactCount(1_200)).toBe("1.2K");
    expect(formatCompactCount(1_000_000)).toBe("1M");
    expect(formatCompactCount(1_250_000)).toBe("1.3M");
  });

  it("marks estimated counts and preserves exact value in the title", () => {
    render(
      <ImportDialog
        busy={false}
        error={null}
        inspection={{ ...csvInspection, rowCount: 1_250_000, rowCountExact: false }}
        onCancel={vi.fn()}
        onClose={vi.fn()}
        onInspectCsv={vi.fn(async () => undefined)}
        onSubmit={vi.fn()}
      />,
    );

    expect(screen.getByLabelText("Source summary")).toHaveTextContent("~1.3M");
    expect(screen.getByTitle("Estimated: 1,250,000 rows")).toBeInTheDocument();
  });

  it("defaults Parquet to link and exposes cancellation while busy", () => {
    const cancel = vi.fn();
    render(
      <ImportDialog
        busy
        error={null}
        inspection={{ ...csvInspection, format: "parquet", csvOptions: null }}
        onCancel={cancel}
        onClose={vi.fn()}
        onInspectCsv={vi.fn(async () => undefined)}
        onSubmit={vi.fn()}
      />,
    );

    expect(screen.getByRole("radio", { name: /Link file/ })).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "Cancel operation" }));
    expect(cancel).toHaveBeenCalledOnce();
  });
});
