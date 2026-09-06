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

  it("opens without scanning and discloses bounded scan cost", () => {
    renderWorkspace();
    expect(executeProfile).not.toHaveBeenCalled();
    expect(screen.getByText("No scan runs when this workspace opens.")).toBeInTheDocument();
    expect(screen.getByText(/returns only bounded summaries/)).toBeInTheDocument();
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

  it("renders provenance, unavailable reasons, and bounded values", async () => {
    renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    expect(
      await screen.findByText("Approximate", { selector: ".profile-provenance" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Not applicable")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.getByText(/Numeric\/temporal summaries are unavailable/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show 1 bounded values" }));
    expect(screen.getByText(/"value": "ok"/)).toBeInTheDocument();
  });

  it("offers a prefilled check handoff without saving or executing a check", async () => {
    const onCreateCheck = vi.fn();
    renderWorkspace(onCreateCheck);
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    const action = await screen.findByRole("button", { name: /Create check from note NULL count/ });
    fireEvent.click(action);
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

  it("rejects a stale opened catalog without starting a scan", () => {
    render(
      <ProfileWorkspace
        catalog={{ ...catalog, revision: "catalog-2" }}
        object={catalog.objects[0]}
        openedCatalogRevision="catalog-1"
        onClose={vi.fn()}
        onCreateCheck={vi.fn()}
        project={project}
        source={null}
        sourceChanged={false}
      />,
    );
    expect(screen.getByText(/catalog or linked source changed/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run profile" })).toBeDisabled();
    expect(executeProfile).not.toHaveBeenCalled();
  });

  it("bounds the initial selection and request to 100 columns", async () => {
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
        project={project}
        source={null}
        sourceChanged={false}
      />,
    );
    expect(screen.getByText("100 of 101 selected · maximum 100")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    await waitFor(() => expect(executeProfile).toHaveBeenCalled());
    expect(vi.mocked(executeProfile).mock.calls[0][0].columns).toHaveLength(100);
  });

  it("surfaces a terminal error and allows an explicit retry", async () => {
    vi.mocked(getProfileStatus).mockResolvedValue({
      ...queued,
      state: "failed",
      error: { code: "profile.catalog_stale", message: "Catalog changed." },
    });
    renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Run profile" }));
    expect(await screen.findByText("Profile did not complete.")).toBeInTheDocument();
    expect(screen.getAllByText("Catalog changed.")).toHaveLength(2);
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
