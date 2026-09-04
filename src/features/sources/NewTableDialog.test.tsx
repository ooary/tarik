import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createTable } from "../../lib/commands";
import { NewTableDialog, validateTableDefinition } from "./NewTableDialog";

vi.mock("../../lib/commands", () => ({ createTable: vi.fn() }));

describe("NewTableDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(createTable).mockResolvedValue(true);
  });

  it("stays disabled without a project", () => {
    render(<NewTableDialog onCreated={vi.fn()} projectId="" />);
    expect(screen.getByRole("button", { name: "New table" })).toBeDisabled();
  });

  it("validates duplicate columns before invoking DuckDB", () => {
    expect(
      validateTableDefinition({
        name: "orders",
        columns: [
          { name: "ID", dataType: "BIGINT", nullable: false },
          { name: "id", dataType: "VARCHAR", nullable: true },
        ],
      }),
    ).toMatch(/duplicated/);
  });

  it("creates one safely shaped table and refreshes catalog", async () => {
    const onCreated = vi.fn();
    render(<NewTableDialog onCreated={onCreated} projectId="project-1" />);
    fireEvent.click(screen.getByRole("button", { name: "New table" }));
    fireEvent.change(screen.getByLabelText("Table name"), {
      target: { value: "order summary" },
    });
    fireEvent.change(screen.getByLabelText("Column 1 name"), { target: { value: "select" } });
    fireEvent.change(screen.getByLabelText("Column 1 type"), { target: { value: "BIGINT" } });
    fireEvent.click(screen.getByLabelText("Column 1 allows NULL"));
    fireEvent.click(screen.getByRole("button", { name: "Add column" }));
    fireEvent.change(screen.getByLabelText("Column 2 name"), {
      target: { value: "net value" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create table" }));

    await waitFor(() =>
      expect(createTable).toHaveBeenCalledWith("project-1", {
        name: "order summary",
        columns: [
          { name: "select", dataType: "BIGINT", nullable: false },
          { name: "net value", dataType: "VARCHAR", nullable: true },
        ],
      }),
    );
    expect(onCreated).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps the definition visible after an engine failure", async () => {
    vi.mocked(createTable).mockRejectedValue(new Error("table already exists"));
    render(<NewTableDialog onCreated={vi.fn()} projectId="project-1" />);
    fireEvent.click(screen.getByRole("button", { name: "New table" }));
    fireEvent.change(screen.getByLabelText("Table name"), { target: { value: "orders" } });
    fireEvent.change(screen.getByLabelText("Column 1 name"), { target: { value: "id" } });
    fireEvent.click(screen.getByRole("button", { name: "Create table" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("table already exists");
    expect(screen.getByLabelText("Table name")).toHaveValue("orders");
  });
});
