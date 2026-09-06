import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cancelProfile,
  executeProfile,
  getProfileStatus,
  type ProfileStatus,
  type ProjectCatalog,
} from "../../lib/commands";
import { ProfileWorkspace } from "./ProfileWorkspace";

vi.mock("../../lib/commands", () => ({
  executeProfile: vi.fn(),
  getProfileStatus: vi.fn(),
  cancelProfile: vi.fn(),
}));

const catalog: ProjectCatalog = {
  revision: "catalog-1",
  objects: [
    { database: "retail", schema: "main", name: "orders", kind: "table", estimatedRowCount: 3 },
  ],
  columns: [
    {
      database: "retail",
      schema: "main",
      object: "orders",
      name: "id",
      dataType: "BIGINT",
      position: 0,
      nullable: false,
    },
    {
      database: "retail",
      schema: "main",
      object: "orders",
      name: "note",
      dataType: "VARCHAR",
      position: 1,
      nullable: true,
    },
  ],
};
const project = { id: "project-1", name: "Retail", duckdbPath: "/data/retail.duckdb" };
const queued: ProfileStatus = {
  profileId: "profile-1",
  state: "queued",
  durationMs: 0,
  snapshot: null,
  error: null,
};
const succeeded: ProfileStatus = {
  profileId: "profile-1",
  state: "succeeded",
  durationMs: 12,
  error: null,
  snapshot: {
    projectId: project.id,
    target: { database: "retail", schema: "main", name: "orders", kind: "table" },
    catalogRevision: "catalog-1",
    mode: "approximate",
    observedAtUnixMs: 1_700_000_000_000,
    statements: [
      {
        columns: [],
        metricKinds: ["row_count"],
        sql: 'SELECT count(*) FROM "retail"."main"."orders"',
      },
      {
        columns: ["note"],
        metricKinds: ["null_count", "distinct_count"],
        sql: 'SELECT count(*) FILTER (WHERE "note" IS NULL), approx_count_distinct("note") FROM "retail"."main"."orders"',
      },
    ],
    metrics: [
      {
        column: null,
        kind: "row_count",
        value: 3,
        provenance: "exact",
        unavailableReason: null,
        truncated: false,
      },
      {
        column: "note",
        kind: "null_count",
        value: 1,
        provenance: "exact",
        unavailableReason: null,
        truncated: false,
      },
      {
        column: "note",
        kind: "distinct_count",
        value: 2,
        provenance: "approximate",
        unavailableReason: null,
        truncated: false,
      },
      {
        column: "note",
        kind: "common_values",
        value: [{ value: "ok", count: 2 }],
        provenance: "exact",
        unavailableReason: null,
        truncated: false,
      },
      {
        column: "note",
        kind: "minimum",
        value: null,
        provenance: "exact",
        unavailableReason: "Numeric/temporal summaries are unavailable for text columns.",
        truncated: false,
      },
    ],
  },
};

function renderWorkspace(onCreateCheck = vi.fn()) {
  return render(
    <ProfileWorkspace
      catalog={catalog}
      object={catalog.objects[0]}
      openedCatalogRevision="catalog-1"
      onClose={vi.fn()}
      onCreateCheck={onCreateCheck}
      onOpenSql={vi.fn()}
      onRefresh={vi.fn().mockResolvedValue(undefined)}
      project={project}
      source={null}
      sourceChanged={false}
    />,
  );
}

describe("ProfileWorkspace", () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
    vi.mocked(executeProfile).mockResolvedValue(queued);
    vi.mocked(getProfileStatus).mockResolvedValue(succeeded);
    vi.mocked(cancelProfile).mockResolvedValue({ ...queued, state: "cancelled" });
  });

  it("opens without scanning and explains the explicit local scan", () => {
    renderWorkspace();
    expect(executeProfile).not.toHaveBeenCalled();
    expect(screen.getByText("Local, explicit scan")).toBeInTheDocument();
    expect(screen.getByText("No scan runs until you choose Run profile.")).toBeInTheDocument();
    expect(screen.getByText("Distinct counts: Fast estimate")).toBeInTheDocument();
    const scanDetails = screen.getByRole("region", { name: "Local explicit scan details" });
    expect(scanDetails).toHaveTextContent("Explicit start");
    expect(scanDetails).toHaveTextContent("Local execution");
    expect(scanDetails).toHaveTextContent("Temporary results");
  });

  it("fills the pre-run workspace with the three result areas", () => {
    renderWorkspace();
    const state = screen.getByText("Ready to inspect 2 columns").closest(".profile-ready-state");
    expect(state).toBeInTheDocument();
    expect(state).not.toHaveClass("profile-empty");
    expect(screen.getByText(/Results and SQL evidence appear here/)).toBeInTheDocument();
    const resultAreas = screen.getByRole("region", { name: "Profile result areas" });
    expect(resultAreas).toHaveTextContent("Columns");
    expect(resultAreas).toHaveTextContent("Measurements");
    expect(resultAreas).toHaveTextContent("SQL evidence");
  });

  it("captures checkbox state before updating the selected columns", () => {
    renderWorkspace();
    fireEvent.click(screen.getByRole("checkbox", { name: "noteVARCHAR" }));
    expect(screen.getByText("1 column selected")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "noteVARCHAR" }));
    expect(screen.getByText("2 columns selected")).toBeInTheDocument();
  });

  it("shows an animated loading state with live elapsed time", async () => {
    vi.useFakeTimers();
    try {
      vi.mocked(executeProfile).mockResolvedValue(queued);
      vi.mocked(getProfileStatus).mockResolvedValue({ ...queued, state: "running" });
      renderWorkspace();
      fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
      await act(async () => Promise.resolve());
      expect(screen.getByRole("status", { name: "Profile loading" })).toBeInTheDocument();
      expect(screen.getByText("Profile is queued")).toBeInTheDocument();
      expect(screen.getByLabelText("Elapsed time 00:00")).toBeInTheDocument();
      await act(async () => vi.advanceTimersByTimeAsync(1_150));
      expect(screen.getByLabelText("Elapsed time 00:01")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Cancel profile" })).toBeEnabled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("submits immutable catalog identity and approximate mode explicitly", async () => {
    renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    await waitFor(() => expect(executeProfile).toHaveBeenCalledTimes(1));
    expect(executeProfile).toHaveBeenCalledWith({
      projectId: "project-1",
      target: { database: "retail", schema: "main", name: "orders", kind: "table" },
      columns: [
        { name: "id", dataType: "BIGINT" },
        { name: "note", dataType: "VARCHAR" },
      ],
      catalogRevision: "catalog-1",
      mode: "approximate",
    });
  });

  it("groups one column's metrics and renders common values as value-count rows", async () => {
    renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    expect(await screen.findByRole("heading", { name: "note" })).toBeInTheDocument();
    expect(screen.getByText("Completeness")).toBeInTheDocument();
    expect(screen.getByText("Cardinality")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Common values"));
    expect(screen.getByRole("columnheader", { name: "Value" })).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: "ok" })).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: "2" })).toBeInTheDocument();
    expect(screen.getByText(/Numeric\/temporal summaries are unavailable/)).toBeInTheDocument();
  });

  it("shows immutable SQL evidence and opens it without executing", async () => {
    const onOpenSql = vi.fn();
    render(
      <ProfileWorkspace
        catalog={catalog}
        object={catalog.objects[0]}
        openedCatalogRevision="catalog-1"
        onClose={vi.fn()}
        onCreateCheck={vi.fn()}
        onOpenSql={onOpenSql}
        onRefresh={vi.fn().mockResolvedValue(undefined)}
        project={project}
        source={null}
        sourceChanged={false}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    const approximate = await screen.findByText("Approximate", { selector: ".profile-provenance" });
    fireEvent.click(approximate.closest("button")!);
    expect(screen.getByText(/Different non-NULL values/)).toBeInTheDocument();
    expect(screen.getByText(/approx_count_distinct/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy value" })).toHaveClass("profile-copy-value");
    expect(screen.getByRole("button", { name: "Copy SQL" })).toHaveClass("profile-copy-sql");
    expect(screen.getByRole("button", { name: "Open SQL" })).toHaveClass("profile-open-sql");
    fireEvent.click(screen.getByRole("button", { name: "Open SQL" }));
    expect(onOpenSql).toHaveBeenCalledWith(
      expect.stringContaining("approx_count_distinct"),
      "Profile evidence: note",
    );
    expect(executeProfile).toHaveBeenCalledTimes(1);
  });

  it("offers a plain-language check handoff without saving or executing", async () => {
    const onCreateCheck = vi.fn();
    renderWorkspace(onCreateCheck);
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    fireEvent.click(await screen.findByRole("button", { name: "Create not-null check" }));
    expect(onCreateCheck).toHaveBeenCalledWith({
      draft: {
        projectId: "project-1",
        name: "note is not NULL",
        target: {
          database: "retail",
          schema: "main",
          object: "orders",
          columns: ["note"],
        },
        options: { kind: "not_null" },
        nullPolicy: "fail_on_null",
        severity: "warning",
        enabled: true,
      },
      observation: succeeded.snapshot?.metrics[1],
    });
  });

  it("rejects stale setup and refreshes catalog without scanning", async () => {
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    render(
      <ProfileWorkspace
        catalog={{ ...catalog, revision: "catalog-2" }}
        object={catalog.objects[0]}
        openedCatalogRevision="catalog-1"
        onClose={vi.fn()}
        onCreateCheck={vi.fn()}
        onOpenSql={vi.fn()}
        onRefresh={onRefresh}
        project={project}
        source={null}
        sourceChanged={false}
      />,
    );
    expect(screen.getByText(/table or source changed/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run profile" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Refresh setup" }));
    await waitFor(() => expect(onRefresh).toHaveBeenCalledTimes(1));
    expect(executeProfile).not.toHaveBeenCalled();
  });

  it("selects twelve columns by default and still enforces the 100-column cap", async () => {
    const wideCatalog: ProjectCatalog = {
      ...catalog,
      columns: Array.from({ length: 101 }, (_, index) => ({
        database: "retail",
        schema: "main",
        object: "orders",
        name: `column_${index}`,
        dataType: "BIGINT",
        position: index,
        nullable: true,
      })),
    };
    render(
      <ProfileWorkspace
        catalog={wideCatalog}
        object={wideCatalog.objects[0]}
        openedCatalogRevision="catalog-1"
        onClose={vi.fn()}
        onCreateCheck={vi.fn()}
        onOpenSql={vi.fn()}
        onRefresh={vi.fn().mockResolvedValue(undefined)}
        project={project}
        source={null}
        sourceChanged={false}
      />,
    );
    expect(screen.getByText("12 columns selected")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Select up to 100" }));
    expect(screen.getByText("100 columns selected")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    await waitFor(() => expect(executeProfile).toHaveBeenCalled());
    expect(vi.mocked(executeProfile).mock.calls[0][0].columns).toHaveLength(100);
  });

  it("supports arrow navigation across responsive Profile views", async () => {
    renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    const columnsTab = await screen.findByRole("tab", { name: "Columns" });
    columnsTab.focus();
    fireEvent.keyDown(columnsTab, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Metrics" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "Metrics" })).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(screen.getByRole("tab", { name: "Metrics" }), { key: "End" });
    expect(screen.getByRole("tab", { name: "SQL evidence" })).toHaveFocus();
  });

  it("shows each terminal error once and allows an explicit retry", async () => {
    vi.mocked(getProfileStatus).mockResolvedValue({
      ...queued,
      state: "failed",
      error: { code: "profile.catalog_stale", message: "Catalog changed." },
    });
    renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    expect(await screen.findByText("Profile did not complete")).toBeInTheDocument();
    expect(screen.getAllByText("Catalog changed.")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Run profile" })).toBeEnabled();
  });

  it("cancels active work when leaving the workspace", async () => {
    vi.mocked(getProfileStatus).mockResolvedValue(queued);
    const view = renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    await screen.findByRole("button", { name: "Cancel profile" });
    view.unmount();
    await waitFor(() => expect(cancelProfile).toHaveBeenCalledWith("profile-1"));
  });

  it("clears a transient polling error after status recovers", async () => {
    vi.useFakeTimers();
    try {
      vi.mocked(getProfileStatus)
        .mockRejectedValueOnce(new Error("temporary disconnect"))
        .mockResolvedValueOnce(succeeded);
      renderWorkspace();
      fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
      await act(async () => Promise.resolve());
      await act(async () => vi.advanceTimersByTimeAsync(150));
      expect(screen.getByRole("alert")).toHaveTextContent("temporary disconnect");
      await act(async () => vi.advanceTimersByTimeAsync(150));
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
      expect(screen.getByText(/Observed/)).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not overlap status polls", async () => {
    vi.useFakeTimers();
    try {
      let resolvePoll: ((value: ProfileStatus) => void) | undefined;
      vi.mocked(getProfileStatus).mockImplementation(
        () => new Promise((resolve) => (resolvePoll = resolve)),
      );
      renderWorkspace();
      fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
      await act(async () => Promise.resolve());
      await act(async () => vi.advanceTimersByTime(450));
      expect(getProfileStatus).toHaveBeenCalledTimes(1);
      await act(async () => resolvePoll?.(succeeded));
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels an active profile explicitly", async () => {
    vi.mocked(getProfileStatus).mockResolvedValue(queued);
    renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    fireEvent.click(await screen.findByRole("button", { name: "Cancel profile" }));
    await waitFor(() => expect(cancelProfile).toHaveBeenCalledWith("profile-1"));
    expect(await screen.findByText(/Profile cancelled/)).toBeInTheDocument();
  });
});
