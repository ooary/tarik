import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useRef, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Button, ConfirmationDialog, ContextMenu, Dialog, Field, Menu, TextEntryDialog } from ".";

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

  it("submits valid controlled text entry with Enter and restores trigger focus", async () => {
    const submit = vi.fn();
    function Harness() {
      const [open, setOpen] = useState(false);
      const [value, setValue] = useState("");
      const triggerRef = useRef<HTMLButtonElement>(null);
      return (
        <>
          <button onClick={() => setOpen(true)} ref={triggerRef} type="button">
            Rename query
          </button>
          <TextEntryDialog
            description="Choose a local query tab name."
            label="Query tab name"
            onOpenChange={setOpen}
            onSubmit={submit}
            onValueChange={setValue}
            open={open}
            returnFocusRef={triggerRef}
            submitLabel="Rename tab"
            title="Rename query tab"
            value={value}
          />
        </>
      );
    }
    render(<Harness />);

    const trigger = screen.getByRole("button", { name: "Rename query" });
    fireEvent.click(trigger);
    const input = await screen.findByRole("textbox", { name: "Query tab name" });
    await waitFor(() => expect(input).toHaveFocus());
    fireEvent.keyDown(input, { key: "Enter" });
    expect(submit).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "Monthly checks" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(submit).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });

  it("focuses Cancel for destructive confirmation and blocks dismissal while busy", async () => {
    function Harness() {
      const [open, setOpen] = useState(true);
      const [busy, setBusy] = useState(false);
      return (
        <ConfirmationDialog
          busy={busy}
          confirmLabel="Delete project"
          description="This permanently removes the managed project."
          detail="Project “Retail”"
          onConfirm={() => setBusy(true)}
          onOpenChange={setOpen}
          open={open}
          title="Delete project?"
          tone="destructive"
        />
      );
    }
    render(<Harness />);

    const cancel = await screen.findByRole("button", { name: "Cancel" });
    await waitFor(() => expect(cancel).toHaveFocus());
    fireEvent.click(screen.getByRole("button", { name: "Delete project" }));
    expect(screen.getByRole("button", { name: "Delete project…" })).toBeDisabled();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "Delete project?" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close dialog" })).toBeDisabled();
  });

  it("keeps operation input and announces errors", async () => {
    render(
      <TextEntryDialog
        description="Create a folder in this project."
        label="Folder name"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
        onValueChange={vi.fn()}
        open
        operationError="query folder name already exists"
        submitLabel="Create folder"
        title="New folder"
        value="Checks"
      />,
    );

    expect(await screen.findByRole("textbox", { name: "Folder name" })).toHaveValue("Checks");
    expect(screen.getByRole("alert")).toHaveTextContent("query folder name already exists");
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
