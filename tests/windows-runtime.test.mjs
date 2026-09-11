import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  assertRuntimeReport,
  assertWindowsRuntimeHost,
  countStructuredEvents,
  descendantProcessTree,
  directoryBytes,
  hiddenExportStages,
  parseSingleChecksum,
  runtimeFailures,
  summarizeProcessTree,
  WINDOWS_RUNTIME_BUDGETS,
} from "../scripts/windows-runtime.mjs";

test("runtime verifier supports non-destructive manual Windows x64 evidence", () => {
  assert.throws(
    () =>
      assertWindowsRuntimeHost({
        platform: "linux",
        arch: "x64",
        actions: "true",
        runnerTemp: "/tmp/runner",
      }),
    /must run on Windows/,
  );
  assert.throws(
    () =>
      assertWindowsRuntimeHost({
        platform: "win32",
        arch: "arm64",
        actions: "true",
        runnerTemp: "C:\\runner",
      }),
    /requires x64 Node/,
  );
  assert.throws(
    () =>
      assertWindowsRuntimeHost({
        platform: "win32",
        arch: "x64",
        actions: undefined,
        runnerTemp: "C:\\runner",
      }),
    /ephemeral GitHub Actions runner/,
  );
  assert.doesNotThrow(() =>
    assertWindowsRuntimeHost({
      platform: "win32",
      arch: "x64",
      actions: "true",
      runnerTemp: "C:\\runner",
    }),
  );
  assert.doesNotThrow(() =>
    assertWindowsRuntimeHost({
      platform: "win32",
      arch: "x64",
      mode: "manual",
    }),
  );
  assert.throws(
    () =>
      assertWindowsRuntimeHost({
        platform: "win32",
        arch: "x64",
        mode: "unknown",
      }),
    /unknown Windows runtime verification mode/,
  );
});

test("outer checksum parser accepts one portable archive only", () => {
  const digest = "a".repeat(64);
  assert.deepEqual(parseSingleChecksum(`${digest}  Tarik-0.1.0-windows-x64-portable.zip\n`), {
    digest,
    file: "Tarik-0.1.0-windows-x64-portable.zip",
  });
  assert.throws(() => parseSingleChecksum(`${digest} *Tarik.zip\n`), /invalid/);
  assert.throws(
    () => parseSingleChecksum(`${digest}  one.zip\n${digest}  two.zip\n`),
    /exactly one/,
  );
  assert.throws(() => parseSingleChecksum(`${digest}  nested\\Tarik.zip\n`), /invalid/);
});

test("process-tree accounting includes only recursive descendants", () => {
  const records = [
    { pid: 10, parentPid: 1, name: "Tarik.exe", workingSetBytes: 100 },
    { pid: 11, parentPid: 10, name: "msedgewebview2.exe", workingSetBytes: 200 },
    { pid: 12, parentPid: 11, name: "msedgewebview2.exe", workingSetBytes: 300 },
    { pid: 20, parentPid: 1, name: "unrelated.exe", workingSetBytes: 999 },
  ];
  assert.deepEqual(
    descendantProcessTree(records, 10).map((record) => record.pid),
    [10, 11, 12],
  );
  assert.deepEqual(summarizeProcessTree(records, 10), {
    processCount: 3,
    totalWorkingSetBytes: 600,
    processes: [
      { pid: 10, parentPid: 1, name: "Tarik.exe", workingSetBytes: 100 },
      { pid: 11, parentPid: 10, name: "msedgewebview2.exe", workingSetBytes: 200 },
      { pid: 12, parentPid: 11, name: "msedgewebview2.exe", workingSetBytes: 300 },
    ],
  });
  assert.throws(() => summarizeProcessTree(records, 99), /absent/);
  assert.throws(() => descendantProcessTree(records, Number.NaN), /root PID is invalid/);
});

test("directory and export residue checks stay bounded", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "tarik-runtime-test-"));
  try {
    await mkdir(path.join(root, "nested"));
    await writeFile(path.join(root, "one.bin"), new Uint8Array(3));
    await writeFile(path.join(root, "nested", "two.bin"), new Uint8Array(5));
    await writeFile(path.join(root, ".tarik-export-one.tmp"), "stage");
    assert.equal(await directoryBytes(root), 13);
    assert.deepEqual(await hiddenExportStages(root), [".tarik-export-one.tmp"]);
    assert.equal(await directoryBytes(path.join(root, "missing")), 0);
    assert.deepEqual(await hiddenExportStages(path.join(root, "missing")), []);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("startup evidence counts only valid structured startup events", () => {
  const lines = [
    JSON.stringify({ target: "app", event: "startup" }),
    "not-json",
    JSON.stringify({ target: "storage", event: "startup_cleanup" }),
    JSON.stringify({ target: "app", event: "startup" }),
  ];
  assert.equal(countStructuredEvents(`${lines.join("\n")}\n`, "app", "startup"), 2);
  assert.equal(countStructuredEvents(`${lines.join("\n")}\n`, "app", "graceful_shutdown"), 0);
});

function passingReport() {
  return {
    package: { checksumVerified: true },
    prerequisites: { webView2Available: true },
    desktop: {
      launches: [
        { windowObserved: true, responding: true, gracefulExit: true, sidecarProcesses: 0 },
        { windowObserved: true, responding: true, gracefulExit: true, sidecarProcesses: 0 },
      ],
      startupEvents: 2,
      gracefulShutdownEvents: 2,
      metadataCreated: true,
      peakProcessTreeRssBytes: 300 * 1024 * 1024,
    },
    engine: {
      handshake: { engineId: "duckdb", protocolVersion: 2 },
      mainWindowHandle: 0,
      largeResult: { rows: 100_000, firstPageRows: 500, lastPageRows: 500 },
      resultCacheBytesAfterRelease: 0,
      completedExport: {
        state: "succeeded",
        rowsWritten: 250_000,
        filesWritten: 3,
        parts: [{ rows: 100_000 }, { rows: 100_000 }, { rows: 50_000 }],
      },
      cancelledExport: { state: "cancelled", hiddenStagesAfterCancel: [] },
      peakSidecarRssBytes: 120 * 1024 * 1024,
      cleanExit: true,
    },
  };
}

test("runtime report passes only complete bounded evidence", () => {
  const report = passingReport();
  assert.deepEqual(runtimeFailures(report), []);
  assert.doesNotThrow(() => assertRuntimeReport(report));

  report.desktop.peakProcessTreeRssBytes = WINDOWS_RUNTIME_BUDGETS.peakDesktopTreeRssBytes + 1;
  report.engine.cancelledExport.hiddenStagesAfterCancel.push(".tarik-export-leftover");
  report.engine.cleanExit = false;
  report.engine.mainWindowHandle = 42;
  const failures = runtimeFailures(report);
  assert.ok(failures.includes("desktop process-tree memory exceeded budget"));
  assert.ok(failures.includes("cancelled export left hidden stages"));
  assert.ok(failures.includes("sidecar did not exit cleanly"));
  assert.ok(failures.includes("sidecar exposed a top-level window"));
  assert.throws(() => assertRuntimeReport(report), /Windows runtime evidence failed/);

  const unsafeManualReport = passingReport();
  unsafeManualReport.evidenceMode = "manual";
  unsafeManualReport.profile = { preserved: false };
  assert.ok(
    runtimeFailures(unsafeManualReport).includes(
      "manual evidence did not preserve the existing AppData roots",
    ),
  );
});
