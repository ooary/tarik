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
    activeOutputPanel: "flow",
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
  assert.equal(calls[0].command, "create_project");
  assert.equal(calls[1].command, "open_project");
  assert.equal(calls[2].command, "reopen_recent_project");
  assert.equal(calls[4].command, "rename_project");
  assert.equal(calls[5].command, "remove_project");
  assert.equal(calls[8].command, "inspect_project_catalog");
});

test("getAppDirectories invokes the typed path command", async () => {
  const { getAppDirectories } = await loadCommandsModule();
  const expected = {
    dataDir: "/tmp/tarik/data",
    cacheDir: "/tmp/tarik/cache",
    logDir: "/tmp/tarik/logs",
  };
  const calls = [];
  const result = await getAppDirectories(async (command, args) => {
    calls.push({ command, args });
    return expected;
  });

  assert.deepEqual(calls, [{ command: "get_app_directories", args: undefined }]);
  assert.deepEqual(result, expected);
});
