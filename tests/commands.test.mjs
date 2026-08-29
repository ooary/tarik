import assert from "node:assert/strict";
import test from "node:test";
import ts from "typescript";
import vm from "node:vm";
import { readFile } from "node:fs/promises";

async function loadCommandsModule() {
  const source = await readFile(new URL("../src/lib/commands.ts", import.meta.url), "utf8");
  const withoutTauriImport = source.replace(
    'import { invoke } from "@tauri-apps/api/core";\n\n',
    "",
  );
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
