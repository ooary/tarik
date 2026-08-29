import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  Button,
  Dialog,
  EmptyState,
  Field,
  InlineError,
  Menu,
  Skeleton,
  Surface,
  Tooltip,
} from ".";

describe("UI primitives", () => {
  it("exposes button tone without changing native behavior", () => {
    render(<Button tone="primary">Run query</Button>);

    expect(screen.getByRole("button", { name: "Run query" })).toHaveClass("ui-button-primary");
  });

  it("associates field labels, hints, and errors", () => {
    const { rerender } = render(<Field hint="Use a unique local name" label="Table name" />);
    const input = screen.getByRole("textbox", { name: "Table name" });
    expect(input).toHaveAccessibleDescription("Use a unique local name");

    rerender(<Field error="This name already exists" label="Table name" />);
    expect(screen.getByRole("textbox", { name: "Table name" })).toHaveAttribute(
      "aria-invalid",
      "true",
    );
    expect(screen.getByText("This name already exists")).toBeInTheDocument();
  });

  it("opens accessible dialog and menu primitives", async () => {
    render(
      <>
        <Dialog title="Import source" trigger={<Button>Open import</Button>}>
          Import settings
        </Dialog>
        <Menu
          items={[{ label: "Rename" }, { label: "Delete", disabled: true }]}
          label="Query actions"
          trigger={<Button>Actions</Button>}
        />
        <Tooltip content="Run current statement">
          <Button>Run</Button>
        </Tooltip>
      </>,
    );

    screen.getByRole("button", { name: "Open import" }).click();
    expect(await screen.findByRole("dialog", { name: "Import source" })).toBeInTheDocument();
    screen.getByRole("button", { name: "Close dialog" }).click();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    fireEvent.pointerDown(screen.getByRole("button", { name: "Query actions" }), {
      button: 0,
      ctrlKey: false,
    });
    expect(await screen.findByRole("menuitem", { name: "Rename" })).toBeInTheDocument();
  });

  it("provides semantic feedback and surface regions", () => {
    render(
      <>
        <InlineError>The selected file cannot be read.</InlineError>
        <EmptyState description="Import CSV or link Parquet to begin." title="No sources yet" />
        <Skeleton label="Loading schema" lines={2} />
        <Surface title="Source settings">Content</Surface>
      </>,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("selected file cannot be read");
    expect(screen.getByText("No sources yet")).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Loading schema" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Source settings" })).toBeInTheDocument();
  });
});
