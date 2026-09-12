import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ActivityWorkspace } from "./ActivityWorkspace";

const commands = vi.hoisted(() => ({
  getAgentAnalysisLimits: vi.fn(),
  setAgentAnalysisLimits: vi.fn(),
  getActivitySnapshot: vi.fn(),
  getAgentQueryDetail: vi.fn(),
  getDesktopQueryDetail: vi.fn(),
  cancelAgentActivityQuery: vi.fn(),
  cancelDesktopActivityQuery: vi.fn(),
  releaseAgentActivityResult: vi.fn(),
  releaseAgentResults: vi.fn(),
}));

vi.mock("../../lib/commands", () => commands);

const snapshot = {
  projectId: "project-1",
  desktopQueries: [],
  agentQueries: [
    {
      clientProfileId: "client-1",
      clientName: "Claude Desktop",
      executionId: "agent-execution-1",
      projectId: "project-1",
      originConnectionId: "connection-1",
      state: "succeeded",
      queueWaitMs: 120,
      runningMs: 880,
      resultId: "agent-execution-1",
      rows: 5_000,
      rowTotalExact: false,
      browseLimitReached: true,
      cacheBytes: 2_048,
      slotHeld: false,
      cancellationRequested: false,
      cleanupPending: false,
      limits: {
        browseRowCap: 5_000,
        maximumResultBytes: 33_554_432,
        retainedResultLimit: 8,
        outstandingQueryLimit: 4,
        profileCacheBytes: 134_217_728,
        globalCacheBytes: 536_870_912,
        queueDeadlineSeconds: 60,
        executionDeadlineSeconds: 60,
      },
    },
  ],
  agentConnections: [
    {
      clientProfileId: "client-1",
      clientName: "Claude Desktop",
      paired: true,
      connected: true,
      connectionId: "connection-1",
      authenticated: true,
      connectedForMs: 10_000,
      lastHeartbeatMsAgo: 1_000,
      heartbeatStale: false,
      projectIds: ["project-1"],
      queuedQueries: 0,
      runningQueries: 0,
      retainedResults: 1,
      retainedCacheBytes: 2_048,
      adapterPid: null,
      lastConnectedAt: "2026-09-12T00:00:00Z",
    },
  ],
  resources: { memoryLimitMib: 8_192, threads: 4, preset: "balanced" },
  progressAvailable: false,
};

describe("ActivityWorkspace", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    Object.values(commands).forEach((mock) => mock.mockReset());
    commands.getAgentAnalysisLimits.mockResolvedValue(snapshot.agentQueries[0].limits);
    commands.setAgentAnalysisLimits.mockResolvedValue(snapshot.agentQueries[0].limits);
    commands.getActivitySnapshot.mockResolvedValue(snapshot);
    commands.getDesktopQueryDetail.mockResolvedValue({
      executionId: "desktop-execution-1",
      sql: "SELECT 1",
    });
    commands.getAgentQueryDetail.mockResolvedValue({
      executionId: "agent-execution-1",
      sql: "SELECT id FROM orders LIMIT 100",
    });
    commands.releaseAgentActivityResult.mockResolvedValue(true);
  });

  it("polls bounded summaries only while mounted and loads SQL on demand", async () => {
    const openSql = vi.fn();
    const view = render(
      <ActivityWorkspace onClose={vi.fn()} onOpenSql={openSql} projectId="project-1" />,
    );
    await screen.findByText("Claude Desktop");
    expect(commands.getAgentQueryDetail).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("option"));
    await screen.findByText("SELECT id FROM orders LIMIT 100");
    expect(screen.getByText(/Limited browse result/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open in editor" }));
    expect(openSql).toHaveBeenCalledWith("SELECT id FROM orders LIMIT 100", "Agent query");

    const before = commands.getActivitySnapshot.mock.calls.length;
    view.unmount();
    await act(async () => vi.advanceTimersByTimeAsync(1_500));
    expect(commands.getActivitySnapshot).toHaveBeenCalledTimes(before);
  });

  it("opens a desktop immutable SQL snapshot on demand without polling SQL", async () => {
    commands.getActivitySnapshot.mockResolvedValue({
      ...snapshot,
      desktopQueries: [
        {
          executionId: "desktop-execution-1",
          projectId: "project-1",
          tabId: "tab-1",
          state: "running",
          durationMs: 500,
          rowsProduced: null,
          rowsAffected: null,
          error: null,
          resultId: null,
          rowTotal: null,
        },
      ],
      agentQueries: [],
    });
    const openSql = vi.fn();
    render(<ActivityWorkspace onClose={vi.fn()} onOpenSql={openSql} projectId="project-1" />);
    const row = await screen.findByRole("option");
    expect(commands.getDesktopQueryDetail).not.toHaveBeenCalled();
    fireEvent.click(row);
    await screen.findByText("SELECT 1");
    fireEvent.click(screen.getByRole("button", { name: "Open in editor" }));
    expect(openSql).toHaveBeenCalledWith("SELECT 1", "Activity query");
  });

  it("does not overlap visible refreshes and exposes verified-unavailable PID truth", async () => {
    render(<ActivityWorkspace onClose={vi.fn()} onOpenSql={vi.fn()} projectId="project-1" />);
    await screen.findByText("Claude Desktop");
    fireEvent.click(screen.getByRole("tab", { name: "Agents" }));
    expect(screen.getByText("Adapter PID")).toBeInTheDocument();
    expect(screen.getByText("Authenticated")).toBeInTheDocument();
    expect(screen.getByText("client-1")).toBeInTheDocument();
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
    expect(screen.getByText("1 · 2 KiB")).toBeInTheDocument();
    await waitFor(() => expect(commands.getActivitySnapshot).toHaveBeenCalled());
  });
});
