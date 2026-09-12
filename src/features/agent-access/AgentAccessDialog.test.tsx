import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AgentAccessDialog } from "./AgentAccessDialog";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

const emptyStatus = {
  enabled: false,
  endpointReady: false,
  pairedClients: [],
  pendingPairings: [],
  connectedClients: 0,
};

const guidedSetup = {
  platform: "linux",
  packagedServerReady: true,
  topologyNote: "Each configured MCP host transport owns one adapter child.",
  duplicateDiagnosis: "Native process parentage is unavailable on this host.",
  hosts: [
    {
      kind: "pi",
      displayName: "Pi",
      setupMethod: "guided",
      state: "unsupported",
      version: null,
      detail: "Use the displayed stdio command on this platform.",
      canConfigure: true,
      canRemove: false,
    },
  ],
};

describe("AgentAccessDialog", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") return Promise.resolve(emptyStatus);
      if (command === "get_agent_setup_status") return Promise.resolve(guidedSetup);
      if (command === "list_agent_approvals") return Promise.resolve([]);
      if (command === "set_agent_access_enabled") {
        return Promise.resolve({ enabled: true, endpointReady: true });
      }
      return Promise.resolve(null);
    });
  });

  it("starts disabled with no inferred project grant", async () => {
    render(<AgentAccessDialog project={{ id: "project-1", name: "Retail", duckdbPath: "/x" }} />);
    const trigger = screen.getByRole("button", { name: "Agent access" });
    expect(trigger).toHaveClass("agent-access-trigger");
    fireEvent.click(trigger);

    expect(await screen.findByText("No clients are paired.")).toBeInTheDocument();
    expect(screen.getByText("Expected process topology")).toBeInTheDocument();
    expect(screen.getByText(/host transport owns one adapter child/)).toBeInTheDocument();
    expect(screen.getByText("No pending requests.")).toBeInTheDocument();
    const enable = screen.getByRole("button", { name: "Enable" });
    expect(enable).toHaveClass("agent-access-toggle-enable");
    expect(enable).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(enable);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_agent_access_enabled", { enabled: true }),
    );
  });

  it("shows guided setup without pairing or granting authority", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") return Promise.resolve(emptyStatus);
      if (command === "get_agent_setup_status") return Promise.resolve(guidedSetup);
      if (command === "list_agent_approvals") return Promise.resolve([]);
      if (command === "plan_agent_setup") {
        return Promise.resolve({
          planId: "plan-guided",
          hostKind: "pi",
          hostName: "Pi",
          operation: "configure",
          setupMethod: "guided",
          summary: "Add Tarik manually in Pi.",
          commandPreview: "/opt/Tarik/tarik-mcp --profile pi --label Pi",
          configTarget: null,
          expiresInSeconds: 300,
        });
      }
      return Promise.resolve(null);
    });
    render(<AgentAccessDialog project={null} />);
    fireEvent.click(screen.getByRole("button", { name: "Agent access" }));

    const showSteps = await screen.findByRole("button", { name: "Show steps" });
    fireEvent.click(showSteps);

    const guide = await screen.findByRole("dialog", { name: "Connect Pi" });
    const access = document.querySelector<HTMLElement>(".agent-access-dialog");
    expect(access).not.toBeNull();
    expect(access).toHaveAttribute("role", "dialog");
    expect(access).toHaveAttribute("aria-hidden", "true");
    expect(within(guide).getByText("Add Tarik manually in Pi.")).toBeInTheDocument();
    expect(within(access!).queryByText("Add Tarik manually in Pi.")).not.toBeInTheDocument();
    expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(2);

    fireEvent.click(within(guide).getByRole("button", { name: "Copy command" }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith("/opt/Tarik/tarik-mcp --profile pi --label Pi"),
    );
    expect(within(guide).getByText(/Setup command copied/)).toBeInTheDocument();

    fireEvent.click(within(guide).getByRole("button", { name: "Close" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Connect Pi" })).not.toBeInTheDocument(),
    );
    expect(showSteps).toHaveFocus();
    expect(invoke).not.toHaveBeenCalledWith("approve_agent_pairing", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("set_agent_project_grant", expect.anything());
  });

  it("applies a reviewed one-click plan but does not pair the host", async () => {
    const windowsSetup = {
      platform: "windows",
      packagedServerReady: true,
      topologyNote: "Each configured MCP host transport owns one adapter child.",
      duplicateDiagnosis: "Check host configuration and process parentage.",
      hosts: [
        {
          kind: "claude_code",
          displayName: "Claude Code",
          setupMethod: "official_cli",
          state: "not_configured",
          version: "2.1.231",
          detail: "Installed and ready for reviewed setup.",
          canConfigure: true,
          canRemove: false,
        },
      ],
    };
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") return Promise.resolve(emptyStatus);
      if (command === "get_agent_setup_status") return Promise.resolve(windowsSetup);
      if (command === "list_agent_approvals") return Promise.resolve([]);
      if (command === "plan_agent_setup") {
        return Promise.resolve({
          planId: "plan-1",
          hostKind: "claude_code",
          hostName: "Claude Code",
          operation: "configure",
          setupMethod: "official_cli",
          summary: "Register packaged tarik-mcp.exe for the current user.",
          commandPreview: "claude.exe mcp add --scope user tarik -- tarik-mcp.exe",
          configTarget: null,
          expiresInSeconds: 300,
        });
      }
      if (command === "apply_agent_setup") {
        return Promise.resolve({
          hostKind: "claude_code",
          operation: "configure",
          state: "restart_required",
          message: "Tarik was configured. Restart the host to begin pairing.",
        });
      }
      return Promise.resolve(null);
    });
    render(<AgentAccessDialog project={null} />);
    fireEvent.click(screen.getByRole("button", { name: "Agent access" }));

    fireEvent.click(await screen.findByRole("button", { name: "Review setup" }));
    const review = await screen.findByRole("dialog", { name: "Connect Claude Code" });
    fireEvent.click(within(review).getByRole("button", { name: "Apply setup" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("apply_agent_setup", { planId: "plan-1" }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Connect Claude Code" })).not.toBeInTheDocument(),
    );
    expect(invoke).toHaveBeenCalledWith("get_agent_setup_status");
    expect(invoke).not.toHaveBeenCalledWith("approve_agent_pairing", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("set_agent_project_grant", expect.anything());
  });

  it("keeps setup failures inside the setup popup", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("clipboard denied"));
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") return Promise.resolve(emptyStatus);
      if (command === "get_agent_setup_status") return Promise.resolve(guidedSetup);
      if (command === "list_agent_approvals") return Promise.resolve([]);
      if (command === "plan_agent_setup") {
        return Promise.resolve({
          planId: "plan-guided",
          hostKind: "pi",
          hostName: "Pi",
          operation: "configure",
          setupMethod: "guided",
          summary: "Add Tarik manually in Pi.",
          commandPreview: "/opt/Tarik/tarik-mcp --profile pi --label Pi",
          configTarget: null,
          expiresInSeconds: 300,
        });
      }
      return Promise.resolve(null);
    });
    render(<AgentAccessDialog project={null} />);
    fireEvent.click(screen.getByRole("button", { name: "Agent access" }));
    fireEvent.click(await screen.findByRole("button", { name: "Show steps" }));

    const guide = await screen.findByRole("dialog", { name: "Connect Pi" });
    fireEvent.click(within(guide).getByRole("button", { name: "Copy command" }));
    expect(await within(guide).findByRole("alert")).toHaveTextContent(
      "The command could not be copied",
    );
    const access = document.querySelector<HTMLElement>(".agent-access-dialog");
    expect(access).not.toBeNull();
    expect(access).toHaveAttribute("role", "dialog");
    expect(access).toHaveAttribute("aria-hidden", "true");
    expect(within(access!).queryByRole("alert")).not.toBeInTheDocument();
  });

  it("installs optional Pi guidance without pairing or granting authority", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") return Promise.resolve(emptyStatus);
      if (command === "get_agent_setup_status") return Promise.resolve(guidedSetup);
      if (command === "get_agent_skill_status") {
        return Promise.resolve({
          host: "pi",
          hostName: "Pi",
          state: "available",
          detail: "The reviewed Tarik workflow skill can be installed for Pi.",
          canInstall: true,
          canRemove: false,
        });
      }
      if (command === "list_agent_approvals") return Promise.resolve([]);
      if (command === "plan_agent_skill") {
        return Promise.resolve({
          planId: "skill-plan-1",
          host: "pi",
          hostName: "Pi",
          operation: "install",
          summary: "Install Tarik's reviewed guidance-only MCP workflow skill for Pi.",
          target: "/home/user/.pi/agent/skills/tarik-mcp/SKILL.md",
          expiresInSeconds: 300,
        });
      }
      if (command === "apply_agent_skill") {
        return Promise.resolve({
          host: "pi",
          operation: "install",
          state: "installed",
          message: "Tarik's guidance-only workflow skill was installed.",
        });
      }
      return Promise.resolve(null);
    });
    render(<AgentAccessDialog project={null} />);
    fireEvent.click(screen.getByRole("button", { name: "Agent access" }));

    const reviewSkill = await screen.findByRole("button", { name: "Review skill" });
    fireEvent.click(reviewSkill);
    const review = await screen.findByRole("dialog", { name: "Install Tarik skill" });
    expect(within(review).getByText(/cannot pair a client/i)).toBeInTheDocument();
    expect(within(review).getByText(/Pairing, project grants/)).toBeInTheDocument();
    fireEvent.click(within(review).getByRole("button", { name: "Install skill" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("apply_agent_skill", { planId: "skill-plan-1" }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Install Tarik skill" })).not.toBeInTheDocument(),
    );
    expect(invoke).not.toHaveBeenCalledWith("approve_agent_pairing", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("set_agent_project_grant", expect.anything());
  });

  it("keeps optional guidance failures inside its review popup", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") return Promise.resolve(emptyStatus);
      if (command === "get_agent_setup_status") return Promise.resolve(guidedSetup);
      if (command === "get_agent_skill_status") {
        return Promise.resolve({
          host: "pi",
          hostName: "Pi",
          state: "available",
          detail: "Guidance is available.",
          canInstall: true,
          canRemove: false,
        });
      }
      if (command === "list_agent_approvals") return Promise.resolve([]);
      if (command === "plan_agent_skill") {
        return Promise.resolve({
          planId: "skill-plan-2",
          host: "pi",
          hostName: "Pi",
          operation: "install",
          summary: "Install guidance.",
          target: "/home/user/.pi/agent/skills/tarik-mcp/SKILL.md",
          expiresInSeconds: 300,
        });
      }
      if (command === "apply_agent_skill") return Promise.reject(new Error("foreign skill"));
      return Promise.resolve(null);
    });
    render(<AgentAccessDialog project={null} />);
    fireEvent.click(screen.getByRole("button", { name: "Agent access" }));

    const reviewSkill = await screen.findByRole("button", { name: "Review skill" });
    fireEvent.click(reviewSkill);
    const review = await screen.findByRole("dialog", { name: "Install Tarik skill" });
    fireEvent.click(within(review).getByRole("button", { name: "Install skill" }));
    expect(await within(review).findByRole("alert")).toHaveTextContent("foreign skill");
    expect(review).toBeInTheDocument();
    fireEvent.click(within(review).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(review).not.toBeInTheDocument());
    expect(reviewSkill).toHaveFocus();
  });

  it("pairs only after the visible local action", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") {
        return Promise.resolve({
          ...emptyStatus,
          enabled: true,
          endpointReady: true,
          pendingPairings: [{ id: "pair-1", displayName: "Pi local", expiresInSeconds: 120 }],
        });
      }
      if (command === "get_agent_setup_status") return Promise.resolve(guidedSetup);
      if (command === "list_agent_approvals") return Promise.resolve([]);
      if (command === "approve_agent_pairing") return Promise.resolve("client-1");
      return Promise.resolve(null);
    });
    render(<AgentAccessDialog project={null} />);
    fireEvent.click(screen.getByRole("button", { name: "Agent access" }));

    const disable = await screen.findByRole("button", { name: "Disable" });
    expect(disable).toHaveClass("agent-access-toggle-disable");
    expect(disable).toHaveAttribute("aria-pressed", "true");

    const action = screen.getByRole("button", { name: "Pair client" });
    fireEvent.click(action);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("approve_agent_pairing", { pairingId: "pair-1" }),
    );
  });

  it("requires exact typed confirmation before critical approval", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") return Promise.resolve(emptyStatus);
      if (command === "get_agent_setup_status") return Promise.resolve(guidedSetup);
      if (command === "list_agent_approvals") {
        return Promise.resolve([
          {
            id: "approval-1",
            action: "sql",
            clientName: "Pi",
            projectId: "project-1",
            projectName: "Retail",
            sql: "DELETE FROM orders",
            decision: "critical_confirmation",
            reasonCode: "agent.unfiltered_delete_critical",
            affectedObjects: ["orders"],
            hasTopLevelFilter: false,
            snapshotHash: "0123456789abcdef",
            criticalPhrase: "APPROVE A1B2C3D4",
            expiresInSeconds: 90,
          },
        ]);
      }
      if (command === "decide_agent_approval") return Promise.resolve(true);
      return Promise.resolve(null);
    });
    render(<AgentAccessDialog project={{ id: "project-1", name: "Retail", duckdbPath: "/x" }} />);
    fireEvent.click(screen.getByRole("button", { name: "Agent access" }));

    const approve = await screen.findByRole("button", { name: "Approve once" });
    expect(approve).toBeDisabled();
    const confirmation = screen.getByRole("textbox");
    const paste = new Event("paste", { bubbles: true, cancelable: true });
    confirmation.dispatchEvent(paste);
    expect(paste.defaultPrevented).toBe(true);
    fireEvent.change(confirmation, { target: { value: "APPROVE A1B2C3D4" } });
    expect(approve).toBeEnabled();
    fireEvent.click(approve);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("decide_agent_approval", {
        approvalId: "approval-1",
        approve: true,
        typedPhrase: "APPROVE A1B2C3D4",
      }),
    );
  });

  it("labels export approval authority separately from SQL mutation", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") return Promise.resolve(emptyStatus);
      if (command === "get_agent_setup_status") return Promise.resolve(guidedSetup);
      if (command === "list_agent_approvals") {
        return Promise.resolve([
          {
            id: "approval-export",
            action: "export",
            clientName: "Pi",
            projectId: "project-1",
            projectName: "Retail",
            sql: "SELECT * FROM orders",
            decision: "critical_confirmation",
            reasonCode: "agent.export_replace_critical",
            affectedObjects: ["Daily exports"],
            hasTopLevelFilter: null,
            snapshotHash: "0123456789abcdef",
            criticalPhrase: "APPROVE E1E2E3E4",
            expiresInSeconds: 90,
          },
        ]);
      }
      return Promise.resolve(null);
    });
    render(<AgentAccessDialog project={{ id: "project-1", name: "Retail", duckdbPath: "/x" }} />);
    fireEvent.click(screen.getByRole("button", { name: "Agent access" }));

    expect(await screen.findByText("Critical export replacement")).toBeInTheDocument();
    expect(screen.getByText("Destination")).toBeInTheDocument();
    expect(screen.getByText("Daily exports")).toBeInTheDocument();
    expect(screen.getByLabelText("Exact SQL snapshot")).toHaveTextContent("SELECT * FROM orders");
  });

  it("captures checkbox values synchronously and sends a project grant", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_agent_access_status") {
        return Promise.resolve({
          ...emptyStatus,
          enabled: true,
          endpointReady: true,
          pairedClients: [
            {
              id: "client-1",
              displayName: "Claude Desktop",
              connected: true,
              lastConnectedAt: null,
              grants: [],
            },
          ],
        });
      }
      if (command === "get_agent_setup_status") return Promise.resolve(guidedSetup);
      if (command === "list_agent_approvals") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    render(<AgentAccessDialog project={{ id: "project-1", name: "Retail", duckdbPath: "/x" }} />);
    fireEvent.click(screen.getByRole("button", { name: "Agent access" }));

    const analyze = await screen.findByRole("checkbox", { name: "Analyze data" });
    fireEvent.click(analyze);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_agent_project_grant", {
        clientId: "client-1",
        grant: {
          projectId: "project-1",
          inspect: true,
          analyze: true,
          modifyWorkspace: false,
          modifyData: false,
        },
      }),
    );
  });
});
