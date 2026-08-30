import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SourceInspection } from "../../lib/commands";
import { ImportDialog } from "./ImportDialog";

const csvInspection: SourceInspection = {
  path: "/data/orders.csv",
  format: "csv",
  suggestedName: "orders",
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
