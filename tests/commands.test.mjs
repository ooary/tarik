import assert from "node:assert/strict";
import test from "node:test";
import ts from "typescript";
import vm from "node:vm";
import { readFile } from "node:fs/promises";

async function loadCommandsModule() {
  const source = await readFile(new URL("../src/lib/commands.ts", import.meta.url), "utf8");
  const withoutTauriImport = source
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
