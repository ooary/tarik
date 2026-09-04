import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Button, ContextMenu, Dialog, Field, Menu } from ".";

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

  it("opens project context actions on right-click", async () => {
    const rename = vi.fn();
    render(
      <ContextMenu
        items={[
          { label: "Rename", onSelect: rename },
          { danger: true, label: "Delete project", onSelect: vi.fn() },
        ]}
        label="Project actions"
      >
        <button type="button">Retail project</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Retail project" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename" }));
    expect(rename).toHaveBeenCalledOnce();
  });
});
