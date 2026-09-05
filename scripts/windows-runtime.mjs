import { readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";

export const WINDOWS_RUNTIME_BUDGETS = Object.freeze({
  peakDesktopTreeRssBytes: 768 * 1024 * 1024,
  peakSidecarRssBytes: 512 * 1024 * 1024,
  resultCacheBytesAfterRelease: 0,
  hiddenExportStagesAfterCancel: 0,
});

export function assertWindowsRuntimeHost({ platform, arch, actions, runnerTemp, mode = "ci" }) {
  if (platform !== "win32") throw new Error("Windows runtime verification must run on Windows");
  if (arch !== "x64")
    throw new Error(`Windows runtime verification requires x64 Node, found ${arch}`);
  if (mode !== "ci" && mode !== "manual") {
    throw new Error(`unknown Windows runtime verification mode: ${mode}`);
  }
  if (mode === "manual") return;
  if (actions !== "true" || !runnerTemp) {
    throw new Error(
      "Windows runtime verification deletes Tarik AppData and is restricted to an ephemeral GitHub Actions runner",
    );
  }
}

export function parseSingleChecksum(text) {
  const lines = text.trim().split(/\r?\n/);
  if (lines.length !== 1) throw new Error("outer checksum file must contain exactly one entry");
  const match = lines[0].match(/^([a-f0-9]{64}) {2}([^/\\]+)$/);
  if (!match) throw new Error("outer checksum line is invalid");
  return { digest: match[1], file: match[2] };
}

export function descendantProcessTree(records, rootPid) {
  if (!Number.isSafeInteger(rootPid) || rootPid <= 0) throw new Error("root PID is invalid");
  const normalized = records
    .map((record) => ({
      pid: Number(record.pid ?? record.ProcessId),
      parentPid: Number(record.parentPid ?? record.ParentProcessId),
      name: String(record.name ?? record.Name ?? "unknown"),
      executablePath: String(record.executablePath ?? record.ExecutablePath ?? ""),
      workingSetBytes: Number(record.workingSetBytes ?? record.WorkingSetSize ?? 0),
    }))
    .filter(
      (record) =>
        Number.isSafeInteger(record.pid) &&
        record.pid > 0 &&
        Number.isSafeInteger(record.parentPid) &&
        record.parentPid >= 0 &&
        Number.isFinite(record.workingSetBytes) &&
        record.workingSetBytes >= 0,
    );
  const byParent = new Map();
  for (const record of normalized) {
    const children = byParent.get(record.parentPid) ?? [];
    children.push(record);
    byParent.set(record.parentPid, children);
  }

  const found = [];
  const pending = [rootPid];
  const visited = new Set();
  while (pending.length > 0) {
    const pid = pending.shift();
    if (visited.has(pid)) continue;
    visited.add(pid);
    const record = normalized.find((candidate) => candidate.pid === pid);
    if (record) found.push(record);
    for (const child of byParent.get(pid) ?? []) pending.push(child.pid);
  }
  return found;
}

export function summarizeProcessTree(records, rootPid) {
  const processes = descendantProcessTree(records, rootPid);
  if (!processes.some((record) => record.pid === rootPid)) {
    throw new Error(`root process ${rootPid} was absent from the process snapshot`);
  }
  return {
    processCount: processes.length,
    totalWorkingSetBytes: processes.reduce((total, record) => total + record.workingSetBytes, 0),
    processes: processes
      .map(({ pid, parentPid, name, workingSetBytes }) => ({
        pid,
        parentPid,
        name,
        workingSetBytes,
      }))
      .sort((left, right) => left.pid - right.pid),
  };
}

export async function directoryBytes(root) {
  let total = 0;
  const pending = [root];
  while (pending.length > 0) {
    const directory = pending.shift();
    let entries;
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch (error) {
      if (error?.code === "ENOENT") continue;
      throw error;
    }
    for (const entry of entries) {
      const candidate = path.join(directory, entry.name);
      if (entry.isDirectory()) pending.push(candidate);
      else if (entry.isFile()) total += (await stat(candidate)).size;
      else throw new Error(`runtime evidence directory contains a non-file entry: ${candidate}`);
    }
  }
  return total;
}

export async function hiddenExportStages(root) {
  try {
    return (await readdir(root)).filter((name) => name.startsWith(".tarik-export-")).sort();
  } catch (error) {
    if (error?.code === "ENOENT") return [];
    throw error;
  }
}

export function countStructuredEvents(logText, target, eventName) {
  return logText
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    })
    .filter((event) => event?.target === target && event?.event === eventName).length;
}

export function runtimeFailures(report, budgets = WINDOWS_RUNTIME_BUDGETS) {
  const failures = [];
  if (report.evidenceMode === "manual" && report.profile?.preserved !== true) {
    failures.push("manual evidence did not preserve the existing AppData roots");
  }
  if (report.package?.checksumVerified !== true)
    failures.push("outer package checksum was not verified");
  if (!report.prerequisites?.webView2Available) failures.push("WebView2 Runtime was not available");
  if (report.desktop?.launches?.length !== 2)
    failures.push("desktop did not complete two launches");
  for (const [index, launch] of (report.desktop?.launches ?? []).entries()) {
    if (!launch.windowObserved)
      failures.push(`desktop launch ${index + 1} did not expose a window`);
    if (!launch.responding) failures.push(`desktop launch ${index + 1} was not responding`);
    if (!launch.gracefulExit) failures.push(`desktop launch ${index + 1} did not exit gracefully`);
    if ((launch.sidecarProcesses ?? 0) !== 0) {
      failures.push(`desktop launch ${index + 1} started DuckDB without an active project`);
    }
  }
  if ((report.desktop?.startupEvents ?? 0) < 2)
    failures.push("startup log did not record both launches");
  if ((report.desktop?.gracefulShutdownEvents ?? 0) < 2)
    failures.push("log did not record coordinated shutdown for both launches");
  if (!report.desktop?.metadataCreated) failures.push("fresh launch did not create metadata");
  if ((report.desktop?.peakProcessTreeRssBytes ?? 0) <= 0) {
    failures.push("desktop process-tree memory was not sampled");
  } else if (report.desktop.peakProcessTreeRssBytes > budgets.peakDesktopTreeRssBytes) {
    failures.push("desktop process-tree memory exceeded budget");
  }
  if (
    report.engine?.handshake?.engineId !== "duckdb" ||
    report.engine?.handshake?.protocolVersion !== 1
  ) {
    failures.push("packaged sidecar handshake was invalid");
  }
  if ((report.engine?.mainWindowHandle ?? -1) !== 0) {
    failures.push("sidecar exposed a top-level window");
  }
  if (report.engine?.largeResult?.rows !== 100_000)
    failures.push("large-result row count differed");
  if (
    report.engine?.largeResult?.firstPageRows !== 500 ||
    report.engine?.largeResult?.lastPageRows !== 500
  ) {
    failures.push("large-result edge pages differed");
  }
  if (
    (report.engine?.resultCacheBytesAfterRelease ?? Number.POSITIVE_INFINITY) >
    budgets.resultCacheBytesAfterRelease
  ) {
    failures.push("released result cache exceeded budget");
  }
  if (report.engine?.completedExport?.state !== "succeeded")
    failures.push("completed export did not succeed");
  if (report.engine?.completedExport?.rowsWritten !== 250_000)
    failures.push("completed export row count differed");
  if (report.engine?.completedExport?.filesWritten !== 3)
    failures.push("completed export part count differed");
  const partRows = report.engine?.completedExport?.parts?.map((part) => part.rows);
  if (JSON.stringify(partRows) !== JSON.stringify([100_000, 100_000, 50_000])) {
    failures.push("completed export part boundaries differed");
  }
  if (report.engine?.cancelledExport?.state !== "cancelled")
    failures.push("long export did not cancel");
  if (
    (report.engine?.cancelledExport?.hiddenStagesAfterCancel?.length ?? Number.POSITIVE_INFINITY) >
    budgets.hiddenExportStagesAfterCancel
  ) {
    failures.push("cancelled export left hidden stages");
  }
  if ((report.engine?.peakSidecarRssBytes ?? 0) <= 0) {
    failures.push("sidecar memory was not sampled");
  } else if (report.engine.peakSidecarRssBytes > budgets.peakSidecarRssBytes) {
    failures.push("sidecar memory exceeded budget");
  }
  if (!report.engine?.cleanExit) failures.push("sidecar did not exit cleanly");
  return failures;
}

export function assertRuntimeReport(report, budgets = WINDOWS_RUNTIME_BUDGETS) {
  const failures = runtimeFailures(report, budgets);
  if (failures.length > 0)
    throw new Error(`Windows runtime evidence failed:\n- ${failures.join("\n- ")}`);
}

export async function readJson(file) {
  return JSON.parse(await readFile(file, "utf8"));
}
