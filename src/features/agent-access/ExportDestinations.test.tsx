import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ExportDestinations } from "./ExportDestinations";

const invoke = vi.fn();
const open = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => open(...args),
}));

const client = {
  id: "client-1",
  displayName: "Pi",
  connected: true,
  lastConnectedAt: null,
  grants: [],
};
const project = { id: "project-1", name: "Retail", duckdbPath: "/private/project.duckdb" };
const destination = {
  destinationId: "destination-1",
  label: "Daily exports",
  formats: ["csv", "parquet"],
  maximumRowsPerPart: 1_000_000,
  maximumTotalBytes: 10 * 1024 * 1024 * 1024,
  createNewOnly: true,
  enabled: true,
  ready: true,
  revision: 1,
};

function renderDestinations() {
  render(<ExportDestinations client={client} disabled={false} project={project} />);
  fireEvent.click(screen.getByRole("button", { name: "Export destinations" }));
  return screen.findByRole("dialog", { name: "Export destinations" });
}

describe("ExportDestinations", () => {
  beforeEach(() => {
    invoke.mockReset();
    open.mockReset();
    invoke.mockImplementation((command: string) => {
      if (command === "list_agent_export_destinations") {
        return Promise.resolve({ projectId: project.id, destinations: [] });
      }
      return Promise.resolve(null);
    });
  });

  it("does nothing when native folder selection is cancelled", async () => {
    open.mockResolvedValue(null);
    const dialog = await renderDestinations();

    fireEvent.click(within(dialog).getByRole("button", { name: "Choose folder" }));
    await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    expect(open).toHaveBeenCalledWith({
      multiple: false,
      directory: true,
      title: "Choose delegated export folder",
    });
    expect(invoke).not.toHaveBeenCalledWith("create_agent_export_destination", expect.anything());
    expect(within(dialog).queryByRole("form")).not.toBeInTheDocument();
  });

  it("creates a bounded destination without rendering the selected path", async () => {
    const privatePath = "/home/user/private exports";
    open.mockResolvedValue(privatePath);
    invoke.mockImplementation((command: string) => {
      if (command === "list_agent_export_destinations") {
        return Promise.resolve({ projectId: project.id, destinations: [] });
      }
      if (command === "create_agent_export_destination") return Promise.resolve(destination);
      return Promise.resolve(null);
    });
    const dialog = await renderDestinations();

    fireEvent.click(within(dialog).getByRole("button", { name: "Choose folder" }));
    expect(
      await within(dialog).findByText("Folder selected. Review policy, then create."),
    ).toBeInTheDocument();
    expect(
      within(dialog).queryByText("No export destinations for this client and project."),
    ).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("Display label")).toHaveFocus();
    expect(within(dialog).queryByText(privatePath)).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent(privatePath);

    fireEvent.change(within(dialog).getByLabelText("Display label"), {
      target: { value: "Team extracts" },
    });
    fireEvent.change(within(dialog).getByLabelText("Rows per part"), {
      target: { value: "250000" },
    });
    fireEvent.change(within(dialog).getByLabelText("Total limit (GiB)"), {
      target: { value: "4" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create destination" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("create_agent_export_destination", {
        clientId: client.id,
        projectId: project.id,
        selectedDirectory: privatePath,
        policy: {
          displayLabel: "Team extracts",
          allowCsv: true,
          allowParquet: true,
          maximumRowsPerPart: 250_000,
          maximumTotalBytes: 4 * 1024 * 1024 * 1024,
        },
      }),
    );
    expect(await within(dialog).findByText("Daily exports")).toBeInTheDocument();
    expect(
      within(dialog).queryByText("No export destinations for this client and project."),
    ).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent(privatePath);
  });

  it("explains invalid destination policy with accessible error text", async () => {
    open.mockResolvedValue("/home/user/exports");
    const dialog = await renderDestinations();

    fireEvent.click(within(dialog).getByRole("button", { name: "Choose folder" }));
    const csv = await within(dialog).findByRole("checkbox", { name: "CSV" });
    const parquet = within(dialog).getByRole("checkbox", { name: "Parquet" });
    fireEvent.click(csv);
    fireEvent.click(parquet);

    const error = within(dialog).getByRole("alert", { name: "" });
    expect(error).toHaveTextContent("Select at least one format.");
    expect(error).toHaveClass("ui-field-error");
    expect(within(dialog).getByRole("group", { name: "Allowed formats" })).toHaveAttribute(
      "aria-invalid",
      "true",
    );
    expect(within(dialog).getByRole("button", { name: "Create destination" })).toBeDisabled();
    expect(invoke).not.toHaveBeenCalledWith("create_agent_export_destination", expect.anything());
  });

  it("shows only redacted policy facts and binds management actions", async () => {
    let listed = { ...destination, ready: false };
    open.mockResolvedValue("/private/repaired");
    invoke.mockImplementation((command: string) => {
      if (command === "list_agent_export_destinations") {
        return Promise.resolve({ projectId: project.id, destinations: [listed] });
      }
      if (command === "repair_agent_export_destination") {
        listed = { ...listed, ready: true, revision: 2 };
        return Promise.resolve(listed);
      }
      if (command === "set_agent_export_destination_enabled") {
        listed = { ...listed, enabled: false, revision: 3 };
        return Promise.resolve(listed);
      }
      if (command === "revoke_agent_export_destination") {
        listed = { ...listed, enabled: false };
        return Promise.resolve(true);
      }
      return Promise.resolve(null);
    });
    const dialog = await renderDestinations();

    expect(await within(dialog).findByText("Daily exports")).toBeInTheDocument();
    expect(within(dialog).getByText("Repair required")).toBeInTheDocument();
    expect(within(dialog).getByText(/CSV \+ PARQUET/)).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("/private/");

    fireEvent.click(within(dialog).getByRole("button", { name: "Repair" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("repair_agent_export_destination", {
        clientId: client.id,
        projectId: project.id,
        destinationId: destination.destinationId,
        selectedDirectory: "/private/repaired",
      }),
    );
    expect(document.body.textContent).not.toContain("/private/repaired");

    const disable = await within(dialog).findByRole("button", { name: "Disable" });
    fireEvent.click(disable);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_agent_export_destination_enabled", {
        clientId: client.id,
        projectId: project.id,
        destinationId: destination.destinationId,
        enabled: false,
      }),
    );

    fireEvent.click(within(dialog).getByRole("button", { name: "Revoke Daily exports" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("revoke_agent_export_destination", {
        clientId: client.id,
        projectId: project.id,
        destinationId: destination.destinationId,
      }),
    );
  });

  it("keeps backend policy errors in the modal and returns focus on close", async () => {
    open.mockResolvedValue("/unsafe/path");
    invoke.mockImplementation((command: string) => {
      if (command === "list_agent_export_destinations") {
        return Promise.resolve({ projectId: project.id, destinations: [] });
      }
      if (command === "create_agent_export_destination") {
        return Promise.reject(new Error("agent.destination_unsafe"));
      }
      return Promise.resolve(null);
    });
    const trigger = screen.queryByRole("button", { name: "Export destinations" });
    expect(trigger).not.toBeInTheDocument();
    const dialog = await renderDestinations();
    const opener = screen.getByRole("button", { name: "Export destinations", hidden: true });

    fireEvent.click(within(dialog).getByRole("button", { name: "Choose folder" }));
    fireEvent.click(await within(dialog).findByRole("button", { name: "Create destination" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("agent.destination_unsafe");
    expect(dialog).toBeInTheDocument();

    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(opener).toHaveFocus();
  });
});
