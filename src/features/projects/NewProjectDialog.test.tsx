import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { NewProjectDialog } from "./NewProjectDialog";

describe("NewProjectDialog", () => {
  it("supports keyboard cancellation and restores trigger focus", async () => {
    render(<NewProjectDialog existingNames={[]} onCreate={vi.fn()} />);
    const trigger = screen.getByRole("button", { name: "New project" });
    fireEvent.click(trigger);
    const input = screen.getByRole("textbox", { name: "Project name" });
    await waitFor(() => expect(input).toHaveFocus());
    fireEvent.keyDown(input, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });

  it("trims submission and locks duplicate recent names", async () => {
    const onCreate = vi.fn().mockResolvedValue(undefined);
    render(<NewProjectDialog existingNames={["Existing"]} onCreate={onCreate} />);
    fireEvent.click(screen.getByRole("button", { name: "New project" }));
    const input = screen.getByRole("textbox", { name: "Project name" });
    fireEvent.change(input, { target: { value: " existing " } });
    expect(screen.getByRole("button", { name: "Create project" })).toBeDisabled();
    expect(screen.getByText(/already exists/i)).toBeInTheDocument();
    fireEvent.change(input, { target: { value: "  Revenue  " } });
    fireEvent.submit(input.closest("form")!);
    await waitFor(() => expect(onCreate).toHaveBeenCalledWith("Revenue"));
  });

  it("keeps input and shows backend failures inline", async () => {
    const onCreate = vi.fn().mockRejectedValue(new Error("engine startup failed"));
    render(<NewProjectDialog existingNames={[]} onCreate={onCreate} />);
    fireEvent.click(screen.getByRole("button", { name: "New project" }));
    const input = screen.getByRole("textbox", { name: "Project name" });
    fireEvent.change(input, { target: { value: "Finance" } });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    expect(await screen.findByText("engine startup failed")).toBeInTheDocument();
    expect(input).toHaveValue("Finance");
  });
});
