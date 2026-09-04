import assert from "node:assert/strict";
import test from "node:test";
import ts from "typescript";
import vm from "node:vm";
import { readFile } from "node:fs/promises";

async function loadCommandsModule() {
  const source = await readFile(new URL("../src/lib/commands.ts", import.meta.url), "utf8");
  const withoutTauriImport = source
    .replace('import { open as openFileDialog } from "@tauri-apps/plugin-dialog";\n', "")
    .replace('import { invoke } from "@tauri-apps/api/core";\n', "")
    .replace('import type { WorkbenchPreferences } from "../app/preferences";\n', "");
  const javascript = ts.transpile(withoutTauriImport, {
    module: ts.ModuleKind.CommonJS,
    target: ts.ScriptTarget.ES2022,
  });
  const module = { exports: {} };
  vm.runInNewContext(javascript, { module, exports: module.exports, Promise });
  return module.exports;
}

test("getRuntimeInfo invokes the typed Tauri command", async () => {
  const { getRuntimeInfo } = await loadCommandsModule();
  const expected = { appName: "Tarik", appVersion: "0.1.0", rustTarget: "linux" };
  const calls = [];
  const result = await getRuntimeInfo(async (command, args) => {
    calls.push({ command, args });
    return expected;
  });

  assert.deepEqual(calls, [{ command: "get_runtime_info", args: undefined }]);
  assert.deepEqual(result, expected);
});

test("workbench preferences use typed metadata commands", async () => {
  const { getWorkbenchPreferences, setWorkbenchPreferences } = await loadCommandsModule();
  const preference = {
    theme: "dark",
    sidebarWidth: 280,
    bottomPanelHeight: 300,
    sidebarOpen: true,
    bottomPanelOpen: true,
  };
  const calls = [];
  const invoke = async (command, args) => {
    calls.push({ command, args });
    return command === "get_workbench_preferences" ? preference : undefined;
  };

  assert.deepEqual(await getWorkbenchPreferences(invoke), preference);
  await setWorkbenchPreferences(preference, invoke);
  assert.equal(
    JSON.stringify(calls),
    JSON.stringify([
      { command: "get_workbench_preferences" },
      { command: "set_workbench_preferences", args: { preferences: preference } },
    ]),
  );
});

test("project lifecycle and catalog use typed commands", async () => {
  const {
    createProject,
    openProject,
    reopenRecentProject,
    listRecentProjects,
    renameProject,
    removeProject,
    closeProject,
    getActiveProject,
    inspectProjectCatalog,
    createTable,
  } = await loadCommandsModule();
  const calls = [];
  const project = { id: "p1", name: "Retail", duckdbPath: "/data/retail.duckdb" };
  const invoke = async (command, args) => {
    calls.push({ command, args });
    if (command === "close_project") return true;
    if (command === "inspect_project_catalog") return { objects: [], columns: [] };
    return project;
  };

  assert.deepEqual(await createProject("Retail", invoke), project);
  assert.deepEqual(await openProject("Retail", "/data/retail.duckdb", invoke), project);
  assert.deepEqual(await reopenRecentProject("p1", invoke), project);
  assert.deepEqual(await listRecentProjects(invoke), project);
  assert.deepEqual(await renameProject("p1", "Renamed", invoke), project);
  assert.deepEqual(await removeProject("p1", invoke), project);
  assert.equal(await closeProject(invoke), true);
  assert.deepEqual(await getActiveProject(invoke), project);
  assert.deepEqual(await inspectProjectCatalog(invoke), { objects: [], columns: [] });
  await createTable(
    "p1",
    { name: "orders", columns: [{ name: "id", dataType: "BIGINT", nullable: false }] },
    invoke,
  );
  assert.equal(calls[0].command, "create_project");
  assert.equal(calls[1].command, "open_project");
  assert.equal(calls[2].command, "reopen_recent_project");
  assert.equal(calls[4].command, "rename_project");
  assert.equal(calls[5].command, "remove_project");
  assert.equal(calls[8].command, "inspect_project_catalog");
  assert.equal(calls[9].command, "create_table");
});

test("source inspection, link, import, repair, and removal use typed commands", async () => {
  const {
    inspectSourceFile,
    linkParquetSource,
    importSourceTable,
    listSources,
    cancelSourceOperation,
    repairLinkedSource,
    dropCatalogObject,
    removeLinkedSource,
  } = await loadCommandsModule();
  const calls = [];
  const invoke = async (command, args) => {
    calls.push({ command, args });
    return command === "cancel_source_operation" || command === "remove_linked_source" ? true : {};
  };
  const csv = { delimiter: ",", hasHeader: true, nullValue: null, allVarchar: false };
  const options = { tableName: "orders", csv, columnOverrides: [] };

  await inspectSourceFile("/data/orders.csv", csv, invoke);
  await linkParquetSource("/data/orders.parquet", "orders", invoke);
  await importSourceTable("/data/orders.csv", options, invoke);
  await listSources("p1", invoke);
  assert.equal(await cancelSourceOperation(invoke), true);
  await repairLinkedSource("s1", "/data/replacement.parquet", invoke);
  await dropCatalogObject("p1", "project", "main", "orders", "table", invoke);
  assert.equal(await removeLinkedSource("s1", invoke), true);

  assert.deepEqual(
    calls.map((call) => call.command),
    [
      "inspect_source_file",
      "link_parquet_source",
      "import_source_table",
      "list_sources",
      "cancel_source_operation",
      "repair_linked_source",
      "drop_catalog_object",
      "remove_linked_source",
    ],
  );
});

test("query validation preserves project SQL and revision", async () => {
  const { validateQuery } = await loadCommandsModule();
  const calls = [];
  const expected = { revision: 7, diagnostics: [] };
  const result = await validateQuery("p1", "SELECT 1", 7, async (command, args) => {
    calls.push({ command, args });
    return expected;
  });
  assert.deepEqual(result, expected);
  assert.equal(
    JSON.stringify(calls),
    JSON.stringify([
      { command: "validate_query", args: { projectId: "p1", sql: "SELECT 1", revision: 7 } },
    ]),
  );
});

test("query plans use the typed project/sql/mode command", async () => {
  const { explainQueryPlan } = await loadCommandsModule();
  const calls = [];
  const invoke = async (command, args) => {
    calls.push({ command, args });
    return {
      mode: "explain",
      nodes: [],
      edges: [],
      rootIds: [],
      rawPlan: "",
      fallbackReason: null,
    };
  };

  await explainQueryPlan("p1", "SELECT 1", "explain", invoke);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].command, "explain_query_plan");
  assert.deepEqual({ ...calls[0].args }, { projectId: "p1", sql: "SELECT 1", mode: "explain" });
});

test("saved queries and folders use explicit typed commands", async () => {
  const {
    createSavedQuery,
    updateSavedQuery,
    listSavedQueries,
    deleteSavedQuery,
    createQueryFolder,
    renameQueryFolder,
    listQueryFolders,
    deleteQueryFolder,
  } = await loadCommandsModule();
  const calls = [];
  const draft = {
    projectId: "p1",
    folderId: null,
    name: "Revenue",
    sqlText: "SELECT 1",
    tags: ["finance"],
  };
  const invoke = async (command, args) => {
    calls.push({ command, args });
    return command.startsWith("delete_") ? true : {};
  };
  await createSavedQuery(draft, invoke);
  await updateSavedQuery("q1", draft, invoke);
  await listSavedQueries("p1", "rev", invoke);
  assert.equal(await deleteSavedQuery("p1", "q1", invoke), true);
  await createQueryFolder("p1", "Reports", invoke);
  await renameQueryFolder("p1", "f1", "Finance", invoke);
  await listQueryFolders("p1", invoke);
  assert.equal(await deleteQueryFolder("p1", "f1", invoke), true);

  assert.deepEqual(
    calls.map((call) => call.command),
    [
      "create_saved_query",
      "update_saved_query",
      "list_saved_queries",
      "delete_saved_query",
      "create_query_folder",
      "rename_query_folder",
      "list_query_folders",
      "delete_query_folder",
    ],
  );
  assert.deepEqual({ ...calls[1].args }, { id: "q1", draft });
  assert.deepEqual({ ...calls[3].args }, { projectId: "p1", id: "q1" });
});

test("history pages use typed project filters", async () => {
  const { listQueryHistoryPage } = await loadCommandsModule();
  const calls = [];
  const filter = {
    status: "failed",
    search: "orders",
    executedFrom: "2026-01-01T00:00:00Z",
    executedTo: null,
    offset: 20,
    limit: 20,
  };
  await listQueryHistoryPage("p1", filter, async (command, args) => {
    calls.push({ command, args });
    return { entries: [], offset: 20, nextOffset: null };
  });
  assert.equal(
    JSON.stringify(calls),
    JSON.stringify([{ command: "list_query_history_page", args: { projectId: "p1", filter } }]),
  );
});

test("history retention and clear use isolated typed commands", async () => {
  const { applyQueryHistoryRetention, clearQueryHistory } = await loadCommandsModule();
  const calls = [];
  const invoke = async (command, args) => {
    calls.push({ command, args });
    return { deleted: 3, remaining: 2 };
  };
  const policy = { maxCount: 100, maxAgeDays: 30 };
  await applyQueryHistoryRetention("p1", policy, invoke);
  await clearQueryHistory("p1", invoke);
  assert.equal(
    JSON.stringify(calls),
    JSON.stringify([
      { command: "apply_query_history_retention", args: { projectId: "p1", policy } },
      { command: "clear_query_history", args: { projectId: "p1" } },
    ]),
  );
});

test("exports use typed lifecycle commands", async () => {
  const { executeExport, getExportStatus, cancelExport } = await loadCommandsModule();
  const calls = [];
  const options = {
    format: "parquet",
    outputDirectory: "/exports",
    baseName: "orders",
    rowsPerPart: 1000,
    overwrite: "fail_if_exists",
    csv: null,
    parquet: { compression: "snappy" },
  };
  const view = { exportId: "x1", state: "queued" };
  const invoke = async (command, args) => {
    calls.push({ command, args });
    return view;
  };

  await executeExport("p1", "SELECT * FROM orders", options, invoke);
  await getExportStatus("x1", invoke);
  await cancelExport("x1", invoke);
  assert.equal(
    JSON.stringify(calls),
    JSON.stringify([
      {
        command: "execute_export",
        args: { projectId: "p1", sql: "SELECT * FROM orders", options },
      },
      { command: "get_export_status", args: { exportId: "x1" } },
      { command: "cancel_export", args: { exportId: "x1" } },
    ]),
  );
});

test("session snapshots use typed metadata commands", async () => {
  const { saveQuerySession, loadQuerySession } = await loadCommandsModule();
  const snapshot = {
    id: "session-1",
    projectId: "project-1",
    tabs: [{ id: "tab-1", title: "Query", sqlText: "SELECT 1", position: 0, isActive: true }],
  };
  const calls = [];
  const invoke = async (command, args) => {
    calls.push({ command, args });
    return command === "load_query_session" ? snapshot : undefined;
  };

  await saveQuerySession(snapshot, invoke);
  assert.deepEqual(await loadQuerySession("session-1", invoke), snapshot);
  assert.equal(calls[0].command, "save_query_session");
  assert.equal(calls[1].command, "load_query_session");
});

test("logs, incidents, cleanup, and shutdown use typed commands", async () => {
  const {
    getLogInfo,
    reportFrontendIncident,
    getLastSupportIncident,
    clearCache,
    registerShutdownReady,
    completeShutdown,
    revealLogDirectory,
    revealExportPart,
  } = await loadCommandsModule();
  const logInfo = {
    directory: "/tmp/tarik/logs",
    activeFile: "/tmp/tarik/logs/tarik.log",
    maxFileBytes: 2097152,
    retainedFiles: 7,
    available: true,
  };
  const incident = {
    incidentId: "11111111-1111-4111-8111-111111111111",
    summary: "The workbench interface stopped rendering.",
    logDirectory: "/tmp/tarik/logs",
    loggingSucceeded: true,
  };
  const input = {
    incidentId: incident.incidentId,
    kind: "frontend.render",
    message: "Error: render failed",
  };
  const calls = [];
  const invoke = async (command, args) => {
    calls.push({ command, args });
    if (command === "get_log_info") return logInfo;
    if (command === "clear_cache") return { artifactsRemoved: 0, warnings: [] };
    if (command === "complete_shutdown") return { phase: "complete" };
    return incident;
  };
  assert.deepEqual(await getLogInfo(invoke), logInfo);
  assert.deepEqual(await reportFrontendIncident(input, invoke), incident);
  assert.deepEqual(await getLastSupportIncident(invoke), incident);
  assert.deepEqual(await clearCache(invoke), { artifactsRemoved: 0, warnings: [] });
  await registerShutdownReady(invoke);
  assert.deepEqual(await completeShutdown(false, invoke), { phase: "complete" });
  await revealLogDirectory(invoke);
  await revealExportPart("export-1", 2, invoke);

  assert.equal(
    JSON.stringify(calls),
    JSON.stringify([
      { command: "get_log_info", args: undefined },
      { command: "report_frontend_incident", args: { input } },
      { command: "get_last_support_incident", args: undefined },
      { command: "clear_cache", args: undefined },
      { command: "register_shutdown_ready", args: undefined },
      { command: "complete_shutdown", args: { skipDraft: false } },
      { command: "reveal_log_directory", args: undefined },
      { command: "reveal_export_part", args: { exportId: "export-1", partNumber: 2 } },
    ]),
  );
});
