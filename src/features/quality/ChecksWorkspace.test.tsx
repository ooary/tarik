import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cancelQualityFailurePreview,
  cancelQualityRun,
  clearQualityCheckHistory,
  createQualityCheck,
  deleteQualityCheck,
  getQualityCheckHistory,
  getQualityFailurePreviewStatus,
  getQualityRunDetail,
  getQualityRunStatus,
  listLatestQualityRuns,
  listQualityChecks,
  previewQualityCheckSql,
  releaseQualityFailurePreview,
  runQualityCheck,
  runQualitySuite,
  startQualityFailurePreview,
  updateQualityCheck,
  type ProjectCatalog,
  type QualityCheckDefinition,
  type QualityCheckDraft,
  type QualityCheckRun,
  type QualityRunDetail,
  type QualityCheckType,
} from "../../lib/commands";
import type { ProfileCheckPrefill } from "../profile/ProfileWorkspace";
import { ChecksWorkspace } from "./ChecksWorkspace";

vi.mock("../../lib/commands", () => ({
  createQualityCheck: vi.fn(),
  updateQualityCheck: vi.fn(),
  listQualityChecks: vi.fn(),
  listLatestQualityRuns: vi.fn(),
  getQualityCheckHistory: vi.fn(),
  deleteQualityCheck: vi.fn(),
  previewQualityCheckSql: vi.fn(),
  runQualityCheck: vi.fn(),
  runQualitySuite: vi.fn(),
  getQualityRunStatus: vi.fn(),
  cancelQualityRun: vi.fn(),
  getQualityRunDetail: vi.fn(),
  rerunQualityRevision: vi.fn(),
  startQualityFailurePreview: vi.fn(),
  getQualityFailurePreviewStatus: vi.fn(),
  cancelQualityFailurePreview: vi.fn(),
  releaseQualityFailurePreview: vi.fn(),
  clearQualityCheckHistory: vi.fn(),
}));

const project = { id: "project-1", name: "Retail", duckdbPath: "/data/retail.duckdb" };
const catalog: ProjectCatalog = {
  revision: "catalog-1",
  objects: [
    { database: "retail", schema: "main", name: "orders", kind: "table", estimatedRowCount: 3 },
    { database: "retail", schema: "main", name: "customers", kind: "table", estimatedRowCount: 2 },
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
      name: "customer_id",
      dataType: "BIGINT",
      position: 1,
      nullable: true,
    },
    {
      database: "retail",
      schema: "main",
      object: "orders",
      name: "amount",
      dataType: "DECIMAL(18,2)",
      position: 2,
      nullable: true,
    },
    {
      database: "retail",
      schema: "main",
      object: "orders",
      name: "status",
      dataType: "VARCHAR",
      position: 3,
      nullable: true,
    },
    {
      database: "retail",
      schema: "main",
      object: "orders",
      name: "created_at",
      dataType: "TIMESTAMP",
      position: 4,
      nullable: true,
    },
    {
      database: "retail",
      schema: "main",
      object: "customers",
      name: "id",
      dataType: "BIGINT",
      position: 0,
      nullable: false,
    },
  ],
};

function draft(options: QualityCheckDraft["options"] = { kind: "not_null" }): QualityCheckDraft {
  return {
    projectId: project.id,
    name: "Order id required",
    target: {
      database: "retail",
      schema: "main",
      object: "orders",
      columns: options.kind === "not_empty" || options.kind === "custom_sql" ? [] : ["id"],
    },
    options,
    nullPolicy: "fail_on_null",
    severity: "warning",
    enabled: true,
  };
}

function definition(
  options: QualityCheckDraft["options"] = { kind: "not_null" },
): QualityCheckDefinition {
  const value = draft(options);
  return {
    ...value,
    id: "check-1",
    checkType: options.kind,
    latestRevisionId: "revision-1",
    revisionNumber: 1,
    createdAt: "2026-09-06T00:00:00Z",
    updatedAt: "2026-09-06T00:00:00Z",
  };
}

function historyRun(overrides: Partial<QualityCheckRun> = {}): QualityCheckRun {
  return {
    id: "run-history-1",
    projectId: project.id,
    checkId: "check-1",
    revisionId: "revision-1",
    outcome: "fail",
    failureCount: 2,
    durationMs: 12,
    observedAt: "2026-09-06T12:00:00Z",
    errorCode: null,
    createdAt: "2026-09-06T12:00:00Z",
    ...overrides,
  };
}

function runDetail(overrides: Partial<QualityRunDetail> = {}): QualityRunDetail {
  const check = definition();
  return {
    run: historyRun(),
    checkName: check.name,
    revisionNumber: 1,
    currentRevisionNumber: 1,
    definition: check,
    countSql: "SELECT count(*) AS failure_count",
    failureSql: "SELECT * FROM orders WHERE id IS NULL",
    custom: false,
    isLatestRevision: true,
    ...overrides,
  };
}

function renderWorkspace(prefill: ProfileCheckPrefill | null = null) {
  const onOpenSql = vi.fn();
  const onProfileTarget = vi.fn();
  const onRepairTarget = vi.fn().mockResolvedValue(undefined);
  const view = render(
    <ChecksWorkspace
      catalog={catalog}
      onClose={vi.fn()}
      onOpenSql={onOpenSql}
      onProfileTarget={onProfileTarget}
      onRepairTarget={onRepairTarget}
      prefill={prefill}
      project={project}
    />,
  );
  return { ...view, onOpenSql, onProfileTarget, onRepairTarget };
}

async function openNewCheck() {
  fireEvent.click(screen.getByRole("button", { name: "New check" }));
  fireEvent.change(screen.getByLabelText("Check name"), {
    target: { value: "New expectation" },
  });
}

describe("ChecksWorkspace", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listQualityChecks).mockResolvedValue([]);
    vi.mocked(listLatestQualityRuns).mockResolvedValue([]);
    vi.mocked(getQualityCheckHistory).mockResolvedValue({
      entries: [],
      offset: 0,
      nextOffset: null,
    });
    vi.mocked(previewQualityCheckSql).mockImplementation(async (value) => ({
      countSql: `COUNT ${value.options.kind}`,
      failureSql: `FAILURES ${value.options.kind}`,
      custom: value.options.kind === "custom_sql",
    }));
    vi.mocked(createQualityCheck).mockImplementation(async (value) => ({
      ...definition(value.options),
      ...value,
    }));
    vi.mocked(updateQualityCheck).mockImplementation(async (id, value) => ({
      ...definition(value.options),
      ...value,
      id,
      revisionNumber: 2,
    }));
    vi.mocked(deleteQualityCheck).mockResolvedValue(true);
    vi.mocked(runQualityCheck).mockResolvedValue({
      runId: "run-1",
      projectId: project.id,
      checkId: "check-1",
      revisionId: "revision-1",
      state: "queued",
      failureCount: null,
      durationMs: 0,
      sql: "SELECT 0",
      error: null,
      observationScope: "per_check",
    });
    vi.mocked(runQualitySuite).mockResolvedValue([]);
    vi.mocked(getQualityRunDetail).mockResolvedValue(runDetail());
    vi.mocked(getQualityRunStatus).mockResolvedValue(null);
    vi.mocked(cancelQualityRun).mockImplementation(async (runId) => ({
      ...(await runQualityCheck(project.id, "check-1")),
      runId,
      state: "cancelled",
    }));
    vi.mocked(releaseQualityFailurePreview).mockResolvedValue(undefined);
    vi.mocked(cancelQualityFailurePreview).mockResolvedValue(null);
    vi.mocked(clearQualityCheckHistory).mockResolvedValue({ deleted: 1, remaining: 0 });
  });

  it("loads bounded definitions without compiling or executing implicitly", async () => {
    renderWorkspace();
    expect(await screen.findByText("No quality checks yet")).toBeInTheDocument();
    expect(listQualityChecks).toHaveBeenCalledWith("project-1");
    expect(listLatestQualityRuns).toHaveBeenCalledWith("project-1");
    expect(getQualityCheckHistory).toHaveBeenCalledWith("project-1", null, 0, 25);
    expect(previewQualityCheckSql).not.toHaveBeenCalled();
    expect(createQualityCheck).not.toHaveBeenCalled();
    expect(runQualityCheck).not.toHaveBeenCalled();
  });

  it("compiles visible SQL through the backend and saves only on explicit action", async () => {
    renderWorkspace();
    await openNewCheck();
    expect(await screen.findByText("FAILURES not_null")).toBeInTheDocument();
    expect(screen.getByText("COUNT not_null")).toBeInTheDocument();
    expect(previewQualityCheckSql).toHaveBeenCalledWith(
      expect.objectContaining({ name: "New expectation", options: { kind: "not_null" } }),
    );
    expect(createQualityCheck).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(createQualityCheck).toHaveBeenCalledTimes(1));
    expect(runQualityCheck).not.toHaveBeenCalled();
  });

  it("opens generated failure SQL in a new query tab without running it", async () => {
    const { onOpenSql } = renderWorkspace();
    await openNewCheck();
    await screen.findByText("FAILURES not_null");
    fireEvent.click(screen.getByRole("button", { name: "Open SQL" }));
    expect(onOpenSql).toHaveBeenCalledWith("FAILURES not_null", "New expectation failures");
    expect(runQualityCheck).not.toHaveBeenCalled();
  });

  it("consumes a Profile prefill as an unsaved, unexecuted draft", async () => {
    const prefill: ProfileCheckPrefill = {
      draft: draft({ kind: "not_null" }),
      observation: {
        column: "id",
        kind: "null_count",
        value: 1,
        provenance: "exact",
        unavailableReason: null,
        truncated: false,
      },
    };
    renderWorkspace(prefill);
    expect(screen.getByLabelText("Check name")).toHaveValue("Order id required");
    expect(screen.getByText("Unsaved draft")).toBeInTheDocument();
    expect(await screen.findByText("FAILURES not_null")).toBeInTheDocument();
    expect(createQualityCheck).not.toHaveBeenCalled();
    expect(runQualityCheck).not.toHaveBeenCalled();
  });

  it("switches to Runs and presents a started saved check", async () => {
    const saved = definition();
    vi.mocked(listQualityChecks).mockResolvedValue([saved]);
    renderWorkspace();
    fireEvent.click(await screen.findByRole("button", { name: "Run Order id required" }));
    expect(await screen.findByText("Started 1 quality run.")).toHaveAttribute("role", "status");
    expect(screen.getByRole("button", { name: "Runs" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByText("Waiting for DuckDB")).toBeInTheDocument();
  });

  it("reopens durable failed evidence and labels a historical current-data preview", async () => {
    const saved = definition();
    const run = historyRun();
    vi.mocked(listQualityChecks).mockResolvedValue([saved]);
    vi.mocked(listLatestQualityRuns).mockResolvedValue([run]);
    vi.mocked(getQualityCheckHistory).mockResolvedValue({
      entries: [run],
      offset: 0,
      nextOffset: null,
    });
    vi.mocked(getQualityRunDetail).mockResolvedValue(
      runDetail({ revisionNumber: 1, currentRevisionNumber: 3, isLatestRevision: false }),
    );
    renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Runs" }));
    fireEvent.click(await screen.findByRole("button", { name: /Order id required/ }));
    expect(await screen.findByText(/Historical run: revision 1/)).toHaveTextContent(
      "Current definition: revision 3",
    );
    expect(screen.getByText("2 failing rows")).toBeInTheDocument();
    expect(screen.getByText(/current rows, not retained historical rows/)).toBeInTheDocument();
    expect(screen.queryByText(/execution error, not proof/)).not.toBeInTheDocument();
  });

  it("opens and releases a bounded current-data failure preview", async () => {
    const saved = definition();
    const run = historyRun();
    vi.mocked(listQualityChecks).mockResolvedValue([saved]);
    vi.mocked(listLatestQualityRuns).mockResolvedValue([run]);
    vi.mocked(getQualityCheckHistory).mockResolvedValue({
      entries: [run],
      offset: 0,
      nextOffset: null,
    });
    vi.mocked(startQualityFailurePreview).mockResolvedValue({
      resultId: "quality-preview-1",
      projectId: project.id,
      revisionId: "revision-1",
      sql: "SELECT * FROM orders WHERE id IS NULL",
      state: "queued",
    });
    vi.mocked(getQualityFailurePreviewStatus).mockResolvedValue({
      executionId: "quality-preview-1",
      state: "succeeded",
      durationMs: 4,
      rowsProduced: 2,
      rowsAffected: null,
      error: null,
      result: { resultId: "quality-preview-1", rowCount: 2, rowCountExact: true },
    });
    renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Runs" }));
    fireEvent.click(await screen.findByRole("button", { name: /Order id required/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Preview current failures" }));
    await waitFor(() =>
      expect(startQualityFailurePreview).toHaveBeenCalledWith(project.id, "run-history-1"),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Close preview" }));
    await waitFor(() =>
      expect(releaseQualityFailurePreview).toHaveBeenCalledWith("quality-preview-1"),
    );
  });

  it("distinguishes execution errors and offers linked-source repair", async () => {
    const saved = definition();
    const run = historyRun({ outcome: "error", failureCount: null, errorCode: "source.missing" });
    vi.mocked(listQualityChecks).mockResolvedValue([saved]);
    vi.mocked(listLatestQualityRuns).mockResolvedValue([run]);
    vi.mocked(getQualityCheckHistory).mockResolvedValue({
      entries: [run],
      offset: 0,
      nextOffset: null,
    });
    vi.mocked(getQualityRunDetail).mockResolvedValue(runDetail({ run }));
    const { onRepairTarget } = renderWorkspace();
    fireEvent.click(screen.getByRole("button", { name: "Runs" }));
    fireEvent.click(await screen.findByRole("button", { name: /Order id required/ }));
    expect(await screen.findByText(/execution error, not proof/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Repair link" }));
    expect(onRepairTarget).toHaveBeenCalledWith(expect.objectContaining({ object: "orders" }));
  });

  it("requires confirmation before running a saved custom SQL check", async () => {
    const custom = definition({ kind: "custom_sql", sql: "SELECT * FROM orders WHERE id < 0" });
    vi.mocked(listQualityChecks).mockResolvedValue([custom]);
    renderWorkspace();
    fireEvent.click(await screen.findByRole("button", { name: "Run Order id required" }));
    expect(
      await screen.findByRole("dialog", { name: "Run custom quality SQL?" }),
    ).toHaveTextContent("SELECT * FROM orders WHERE id < 0");
    expect(runQualityCheck).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Run custom SQL" }));
    await waitFor(() => expect(runQualityCheck).toHaveBeenCalledWith("project-1", "check-1"));
  });

  it("refuses stale Profile targets before preview, save, or run", () => {
    const stalePrefill: ProfileCheckPrefill = {
      draft: {
        ...draft(),
        target: { database: "retail", schema: "main", object: "removed", columns: ["id"] },
      },
      observation: {
        column: "id",
        kind: "null_count",
        value: 1,
        provenance: "exact",
        unavailableReason: null,
        truncated: false,
      },
    };
    renderWorkspace(stalePrefill);
    expect(screen.getByText("Choose a current table or view.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(previewQualityCheckSql).not.toHaveBeenCalled();
  });

  it.each([
    ["not_empty", "Table is not empty"],
    ["not_null", "Value is not NULL"],
    ["unique", "Key is unique"],
    ["accepted_values", "Value is accepted"],
    ["range", "Value is in range"],
    ["relationship", "Key has a parent"],
    ["freshness", "Timestamp is fresh"],
    ["custom_sql", "Custom read-only SQL"],
  ] satisfies Array<[QualityCheckType, string]>)(
    "offers the %s guided variant",
    async (kind, label) => {
      renderWorkspace();
      fireEvent.click(screen.getByRole("button", { name: "New check" }));
      fireEvent.change(screen.getByLabelText("Expectation"), { target: { value: kind } });
      expect(screen.getByRole("option", { name: label })).toBeInTheDocument();
    },
  );

  it("explains NULL and uniqueness semantics in plain language", async () => {
    renderWorkspace();
    await openNewCheck();
    fireEvent.change(screen.getByLabelText("Expectation"), { target: { value: "unique" } });
    expect(await screen.findByText(/repeated non-NULL keys fail/)).toBeInTheDocument();
    expect(screen.getByText(/Distinct profile counts do not prove/)).toBeInTheDocument();
    expect(screen.getByText("How should NULL be treated?")).toBeInTheDocument();
  });
});
