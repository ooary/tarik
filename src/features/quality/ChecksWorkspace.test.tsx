import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createQualityCheck,
  deleteQualityCheck,
  listLatestQualityRuns,
  listQualityChecks,
  previewQualityCheckSql,
  runQualityCheck,
  updateQualityCheck,
  type ProjectCatalog,
  type QualityCheckDefinition,
  type QualityCheckDraft,
  type QualityCheckType,
} from "../../lib/commands";
import type { ProfileCheckPrefill } from "../profile/ProfileWorkspace";
import { ChecksWorkspace } from "./ChecksWorkspace";

vi.mock("../../lib/commands", () => ({
  createQualityCheck: vi.fn(),
  updateQualityCheck: vi.fn(),
  listQualityChecks: vi.fn(),
  listLatestQualityRuns: vi.fn(),
  deleteQualityCheck: vi.fn(),
  previewQualityCheckSql: vi.fn(),
  runQualityCheck: vi.fn(),
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

function renderWorkspace(prefill: ProfileCheckPrefill | null = null) {
  const onOpenSql = vi.fn();
  const view = render(
    <ChecksWorkspace
      catalog={catalog}
      onClose={vi.fn()}
      onOpenSql={onOpenSql}
      prefill={prefill}
      project={project}
    />,
  );
  return { ...view, onOpenSql };
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
  });

  it("loads bounded definitions without compiling or executing implicitly", async () => {
    renderWorkspace();
    expect(await screen.findByText("No quality checks yet")).toBeInTheDocument();
    expect(listQualityChecks).toHaveBeenCalledWith("project-1");
    expect(listLatestQualityRuns).toHaveBeenCalledWith("project-1");
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
