#!/usr/bin/env node

import { execFile, spawn } from "node:child_process";
import { mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { promisify } from "node:util";
import { assertPortableContents, sha256, verifyPortableChecksums } from "./windows-package.mjs";
import {
  assertRuntimeReport,
  assertWindowsRuntimeHost,
  countStructuredEvents,
  directoryBytes,
  hiddenExportStages,
  parseSingleChecksum,
  readJson,
  summarizeProcessTree,
  WINDOWS_RUNTIME_BUDGETS,
} from "./windows-runtime.mjs";

const execFileAsync = promisify(execFile);
const root = path.resolve(import.meta.dirname, "..");
const stage = path.join(root, "target", "release-artifacts", "windows");
const evidenceMode = process.argv.includes("--manual") ? "manual" : "ci";
const evidenceRoot = path.join(
  root,
  "target",
  evidenceMode === "manual" ? "windows-manual-evidence" : "windows-runtime-evidence",
);
const reportFile = path.join(evidenceRoot, "runtime-report.json");
const identifier = "com.tarik.desktop";
const terminal = new Set(["succeeded", "failed", "cancelled"]);

let runtimeReport = {
  schemaVersion: 1,
  evidenceMode,
  recordedAt: new Date().toISOString(),
  verdict: { automatedPassed: false, failures: ["runtime verification did not complete"] },
};

async function main() {
  try {
    assertWindowsRuntimeHost({
      platform: process.platform,
      arch: process.arch,
      actions: process.env.GITHUB_ACTIONS,
      runnerTemp: process.env.RUNNER_TEMP,
      mode: evidenceMode,
    });
    await rm(evidenceRoot, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
    await mkdir(evidenceRoot, { recursive: true });

    console.log("==> Verify and extract Windows portable candidate");
    const checksum = parseSingleChecksum(await readFile(path.join(stage, "SHA256SUMS"), "utf8"));
    const archive = path.join(stage, checksum.file);
    if ((await sha256(archive)) !== checksum.digest) throw new Error("outer ZIP checksum mismatch");
    const manifest = await readJson(path.join(stage, "release-manifest.json"));
    if (
      manifest.artifacts?.length !== 1 ||
      manifest.artifacts[0]?.file !== checksum.file ||
      manifest.artifacts[0]?.sha256 !== checksum.digest
    ) {
      throw new Error("release manifest does not match the checksummed ZIP");
    }
    const extractedRoot = path.join(evidenceRoot, "extracted");
    await expandArchive(archive, extractedRoot);
    const portableRoot = path.join(extractedRoot, checksum.file.replace(/\.zip$/i, ""));
    await assertPortableContents(portableRoot);
    await verifyPortableChecksums(portableRoot);

    const profile = await windowsProfilePaths();
    const profileState = {
      roamingExistedAtStart: await pathExists(profile.roaming),
      localExistedAtStart: await pathExists(profile.local),
      preserved: evidenceMode === "manual",
    };
    if (evidenceMode === "ci") await resetProfile(profile);
    const baselineLog = await readOptionalText(profile.logFile);
    const baselineStartupEvents = countStructuredEvents(baselineLog, "app", "startup");
    const baselineGracefulShutdownEvents = countStructuredEvents(
      baselineLog,
      "app",
      "graceful_shutdown",
    );
    const existingDesktopProcesses = (await processSnapshot()).filter((record) =>
      ["tarik.exe", "tarik-engine-duckdb.exe"].includes(record.name.toLowerCase()),
    );
    if (existingDesktopProcesses.length > 0) {
      throw new Error("close all Tarik desktop and sidecar processes before manual verification");
    }
    const desktop = {
      launches: [],
      startupEvents: 0,
      gracefulShutdownEvents: 0,
      metadataCreated: false,
      peakProcessTreeRssBytes: 0,
    };

    console.log(
      evidenceMode === "ci"
        ? "==> Launch extracted Tarik.exe with fresh AppData"
        : "==> Launch extracted Tarik.exe without deleting existing AppData",
    );
    desktop.launches.push(await launchDesktop(path.join(portableRoot, "Tarik.exe"), profile));
    console.log("==> Restart extracted Tarik.exe with the same AppData");
    desktop.launches.push(await launchDesktop(path.join(portableRoot, "Tarik.exe"), profile));
    const logText = await readFile(profile.logFile, "utf8");
    desktop.startupEvents =
      countStructuredEvents(logText, "app", "startup") - baselineStartupEvents;
    desktop.gracefulShutdownEvents =
      countStructuredEvents(logText, "app", "graceful_shutdown") - baselineGracefulShutdownEvents;
    desktop.metadataCreated = (await stat(profile.metadata)).size > 0;
    desktop.peakProcessTreeRssBytes = Math.max(
      ...desktop.launches.map((launch) => launch.peakProcessTreeRssBytes),
    );

    console.log("==> Run bounded workload against extracted DuckDB sidecar");
    const engine = await runEngineWorkload(
      path.join(portableRoot, "tarik-engine-duckdb.exe"),
      path.join(evidenceRoot, "engine-workload"),
    );
    const { stdout: osCaption } = await execPowerShell(
      "(Get-CimInstance Win32_OperatingSystem).Caption + ' ' + (Get-CimInstance Win32_OperatingSystem).Version",
    );
    const report = {
      schemaVersion: 1,
      evidenceMode,
      recordedAt: new Date().toISOString(),
      gitRevision: manifest.gitRevision,
      sourceDirty: manifest.sourceDirty,
      machine: {
        os: osCaption.trim(),
        architecture: process.arch,
        node: process.version,
        runnerImage:
          process.env.ImageOS ?? (evidenceMode === "manual" ? "local-manual" : "unknown"),
      },
      profile: profileState,
      package: {
        archive: checksum.file,
        bytes: (await stat(archive)).size,
        sha256: checksum.digest,
        checksumVerified: true,
        extractedContentsVerified: true,
        sourceDirty: manifest.sourceDirty,
        signed: manifest.signed,
      },
      prerequisites: {
        webView2Available: desktop.launches.every((launch) => launch.webView2Processes > 0),
        webView2Version:
          desktop.launches.map((launch) => launch.webView2Version).find(Boolean) ?? null,
        missingRuntimeBehavior:
          "Tauri 2.11.5 release runtime displays a blocking WebView2 prerequisite dialog with the Microsoft download URL; clean-machine missing-runtime review remains manual.",
      },
      budgets: WINDOWS_RUNTIME_BUDGETS,
      desktop,
      engine,
      manualGatesPending: [
        "Windows 10 x64 clean-machine workflow",
        "Windows 11 x64 clean-machine workflow",
        "100/125/150/200 percent DPI visual matrix",
        "mixed-monitor scaling transition",
        "light/dark/system and keyboard-only review",
        "missing-WebView2 clean-machine dialog review",
      ],
    };
    runtimeReport = report;
    assertRuntimeReport(report);
    report.verdict = { automatedPassed: true, failures: [] };
    await writeFile(reportFile, `${JSON.stringify(runtimeReport, null, 2)}\n`);
    if (evidenceMode === "ci") await resetProfile(profile);
    console.log(`Windows runtime evidence: ${reportFile}`);
  } catch (error) {
    const message = error.stack ?? error.message;
    runtimeReport.verdict = { automatedPassed: false, failures: [message] };
    if (process.platform === "win32") {
      await mkdir(evidenceRoot, { recursive: true }).catch(() => undefined);
      await writeFile(reportFile, `${JSON.stringify(runtimeReport, null, 2)}\n`).catch(
        () => undefined,
      );
    }
    console.error(`Windows runtime verification failed: ${message}`);
    process.exitCode = 1;
  }
}

async function windowsProfilePaths() {
  const { stdout } = await execPowerShell(
    "[pscustomobject]@{ roaming = [Environment]::GetFolderPath('ApplicationData'); local = [Environment]::GetFolderPath('LocalApplicationData') } | ConvertTo-Json -Compress",
  );
  const folders = JSON.parse(stdout);
  if (!folders.roaming || !folders.local)
    throw new Error("Windows known AppData folders are unavailable");
  const roaming = path.join(folders.roaming, identifier);
  const local = path.join(folders.local, identifier);
  return {
    roaming,
    local,
    metadata: path.join(roaming, "tarik.sqlite"),
    logFile: path.join(local, "logs", "tarik.log"),
  };
}

async function resetProfile(profile) {
  for (const directory of new Set([profile.roaming, profile.local])) {
    await rm(directory, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
}

async function expandArchive(archive, destination) {
  await rm(destination, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
  await execPowerShell(
    `Expand-Archive -LiteralPath '${quotePowerShell(archive)}' -DestinationPath '${quotePowerShell(destination)}' -Force`,
  );
}

async function launchDesktop(executable, profile) {
  const startedAt = Date.now();
  const childEnvironment =
    evidenceMode === "ci"
      ? { ...process.env, TEMP: process.env.RUNNER_TEMP, TMP: process.env.RUNNER_TEMP }
      : process.env;
  const child = spawn(executable, [], {
    cwd: path.dirname(executable),
    env: childEnvironment,
    shell: false,
    stdio: "ignore",
  });
  const exit = processExit(child);
  const samples = [];
  let windowInfo = null;
  let initialError;
  child.once("error", (error) => (initialError = error));
  try {
    const deadline = Date.now() + 60_000;
    while (Date.now() < deadline) {
      if (initialError) throw initialError;
      if (child.exitCode !== null)
        throw new Error(`Tarik.exe exited during startup with ${child.exitCode}`);
      const [snapshot, observedWindow, metadataExists, logExists] = await Promise.all([
        processSnapshot(),
        processWindow(child.pid),
        fileExists(profile.metadata),
        fileExists(profile.logFile),
      ]);
      let tree;
      try {
        tree = summarizeProcessTree(snapshot, child.pid);
      } catch (error) {
        if (!String(error.message).includes("was absent")) throw error;
        await delay(250);
        continue;
      }
      samples.push({ elapsedMs: Date.now() - startedAt, ...tree });
      if (
        observedWindow?.mainWindowHandle > 0 &&
        observedWindow.responding &&
        metadataExists &&
        logExists
      ) {
        windowInfo = observedWindow;
        break;
      }
      await delay(500);
    }
    if (!windowInfo)
      throw new Error(
        "Tarik.exe did not expose a responding window and startup state in 60 seconds",
      );
    await delay(1_000);
    const finalSnapshot = await processSnapshot();
    const finalTree = summarizeProcessTree(finalSnapshot, child.pid);
    samples.push({ elapsedMs: Date.now() - startedAt, ...finalTree });
    const treePids = new Set(finalTree.processes.map((item) => item.pid));
    const sidecarProcesses = finalTree.processes.filter(
      (item) => item.name.toLowerCase() === "tarik-engine-duckdb.exe",
    ).length;
    const webviews = finalSnapshot.filter(
      (item) =>
        treePids.has(Number(item.pid ?? item.ProcessId)) &&
        String(item.name ?? item.Name ?? "")
          .toLowerCase()
          .includes("msedgewebview2"),
    );
    const webView2Executable = webviews
      .map((item) => item.executablePath ?? item.ExecutablePath)
      .find(Boolean);
    const webView2Version = await executableVersion(webView2Executable);
    const deviceScaleFactors = [
      ...new Set(
        webviews
          .map((item) => String(item.commandLine ?? item.CommandLine ?? ""))
          .map((commandLine) => commandLine.match(/--device-scale-factor=([^\s]+)/)?.[1])
          .filter(Boolean),
      ),
    ].sort();
    const closeRequested = await closeMainWindow(child.pid);
    const result = await Promise.race([exit, delay(20_000).then(() => null)]);
    const gracefulExit = result?.code === 0;
    if (!gracefulExit) {
      if (child.exitCode === null) await terminateTree(child.pid);
      throw new Error(
        `Tarik.exe did not exit gracefully after its window closed (code ${result?.code ?? child.exitCode ?? "timeout"})`,
      );
    }
    await Promise.race([exit, delay(10_000)]);
    return {
      pid: child.pid,
      durationMs: Date.now() - startedAt,
      windowObserved: windowInfo.mainWindowHandle > 0,
      responding: windowInfo.responding,
      closeRequested,
      gracefulExit,
      exitCode: result?.code ?? child.exitCode,
      webView2Processes: webviews.length,
      webView2Version,
      deviceScaleFactors,
      peakProcessTreeRssBytes: Math.max(...samples.map((sample) => sample.totalWorkingSetBytes)),
      peakProcessCount: Math.max(...samples.map((sample) => sample.processCount)),
      sidecarProcesses,
      samples,
    };
  } finally {
    if (child.exitCode === null) {
      await terminateTree(child.pid).catch(() => undefined);
      await Promise.race([exit, delay(10_000)]);
    }
  }
}

async function runEngineWorkload(executable, workloadRoot) {
  await rm(workloadRoot, { recursive: true, force: true });
  const resultRoot = path.join(workloadRoot, "results");
  const completeOutput = path.join(workloadRoot, "complete-export");
  const cancelOutput = path.join(workloadRoot, "cancel-export");
  await Promise.all([
    mkdir(resultRoot, { recursive: true }),
    mkdir(completeOutput, { recursive: true }),
    mkdir(cancelOutput, { recursive: true }),
  ]);
  const client = new EngineClient(executable);
  const memorySamples = [];
  let sampling = true;
  const sampler = (async () => {
    while (sampling && client.child.exitCode === null) {
      const memory = await processWorkingSet(client.child.pid).catch(() => 0);
      memorySamples.push({ elapsedMs: Date.now() - client.startedAt, workingSetBytes: memory });
      await delay(100);
    }
  })();
  try {
    const handshake = await client.request("engine.handshake");
    const windowInfo = await processWindow(client.child.pid);
    await client.request("session.open", {
      sessionId: "windows-runtime",
      locator: {
        engineId: "duckdb",
        payload: { path: path.join(workloadRoot, "runtime.duckdb") },
      },
    });
    await client.request("query.execute", {
      sessionId: "windows-runtime",
      executionId: "large-result",
      sql: "SELECT i, repeat('x', 128) AS payload FROM range(100000) t(i)",
      cacheDir: resultRoot,
    });
    const largeStatus = await client.poll("query", "large-result", 120_000);
    if (largeStatus.state !== "succeeded")
      throw new Error(`large result failed: ${JSON.stringify(largeStatus)}`);
    const firstPage = await client.request("result.get_page", {
      resultId: "large-result",
      offset: 0,
      maxRows: 500,
    });
    const lastPage = await client.request("result.get_page", {
      resultId: "large-result",
      offset: 99_500,
      maxRows: 500,
    });
    const cacheBytesBeforeRelease = await directoryBytes(resultRoot);
    await client.request("result.release", { resultId: "large-result" });
    const resultCacheBytesAfterRelease = await directoryBytes(resultRoot);

    await client.request("export.execute", {
      sessionId: "windows-runtime",
      exportId: "complete-export",
      sql: "SELECT i, repeat('x', 64) AS payload FROM range(250000) t(i)",
      options: {
        format: "csv",
        outputDirectory: completeOutput,
        baseName: "completed",
        rowsPerPart: 100_000,
        overwrite: "fail_if_exists",
        csv: { delimiter: ",", includeHeader: true },
        parquet: null,
      },
    });
    const completedExport = await client.poll("export", "complete-export", 180_000);
    if (completedExport.state !== "succeeded")
      throw new Error(`completed export failed: ${JSON.stringify(completedExport)}`);

    await client.request("export.execute", {
      sessionId: "windows-runtime",
      exportId: "cancel-export",
      sql: "SELECT i, repeat('x', 512) AS payload FROM range(1000000000) t(i)",
      options: {
        format: "csv",
        outputDirectory: cancelOutput,
        baseName: "cancelled",
        rowsPerPart: 100_000,
        overwrite: "fail_if_exists",
        csv: { delimiter: ",", includeHeader: true },
        parquet: null,
      },
    });
    await waitUntilRunning(client, "cancel-export", 15_000);
    await delay(100);
    await client.request("export.cancel", { exportId: "cancel-export" });
    const cancelledExport = await client.poll("export", "cancel-export", 120_000);
    await client.request("result.release_all");
    await client.request("session.close", { sessionId: "windows-runtime" });
    client.closeInput();
    const exitResult = await Promise.race([client.exit, delay(10_000).then(() => null)]);
    const cleanExit = exitResult?.code === 0;
    return {
      handshake,
      mainWindowHandle: windowInfo?.mainWindowHandle ?? 0,
      dataset: {
        largeResultRows: 100_000,
        completedExportRows: 250_000,
        cancelledExportRowsRequested: 1_000_000_000,
      },
      largeResult: {
        state: largeStatus.state,
        rows: largeStatus.rowsProduced,
        firstPageRows: firstPage.rows.length,
        lastPageRows: lastPage.rows.length,
        cacheBytesBeforeRelease,
      },
      resultCacheBytesAfterRelease,
      completedExport: {
        state: completedExport.state,
        rowsWritten: completedExport.rowsWritten,
        filesWritten: completedExport.filesWritten,
        parts: completedExport.completedParts.map(({ partNumber, rows, bytes }) => ({
          partNumber,
          rows,
          bytes,
        })),
        outputBytes: await directoryBytes(completeOutput),
      },
      cancelledExport: {
        state: cancelledExport.state,
        rowsWritten: cancelledExport.rowsWritten,
        filesWritten: cancelledExport.filesWritten,
        outputBytes: await directoryBytes(cancelOutput),
        hiddenStagesAfterCancel: await hiddenExportStages(cancelOutput),
      },
      peakSidecarRssBytes: Math.max(...memorySamples.map((sample) => sample.workingSetBytes), 0),
      memorySamples,
      cleanExit,
    };
  } finally {
    sampling = false;
    await sampler;
    if (client.child.exitCode === null) {
      client.closeInput();
      await Promise.race([client.exit, delay(2_000)]);
    }
    if (client.child.exitCode === null)
      await terminateTree(client.child.pid).catch(() => undefined);
  }
}

class EngineClient {
  constructor(executable) {
    this.startedAt = Date.now();
    this.sequence = 0;
    this.buffer = "";
    this.pending = new Map();
    this.stderr = "";
    this.child = spawn(executable, [], {
      cwd: path.dirname(executable),
      shell: false,
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.exit = processExit(this.child);
    this.child.stdout.setEncoding("utf8");
    this.child.stderr.setEncoding("utf8");
    this.child.stdout.on("data", (chunk) => this.receive(chunk));
    this.child.stderr.on("data", (chunk) => (this.stderr += chunk));
    this.child.once("error", (error) => this.rejectAll(error));
    this.child.once("exit", (code) => {
      if (this.pending.size > 0)
        this.rejectAll(new Error(`engine exited with ${code}: ${this.stderr.slice(-2000)}`));
    });
  }

  receive(chunk) {
    this.buffer += chunk;
    while (this.buffer.includes("\n")) {
      const newline = this.buffer.indexOf("\n");
      const line = this.buffer.slice(0, newline);
      this.buffer = this.buffer.slice(newline + 1);
      if (!line.trim()) continue;
      let response;
      try {
        response = JSON.parse(line);
      } catch (error) {
        this.rejectAll(new Error(`invalid engine JSON: ${error.message}`));
        continue;
      }
      const pending = this.pending.get(response.id);
      if (!pending) continue;
      this.pending.delete(response.id);
      clearTimeout(pending.timeout);
      if (response.ok) pending.resolve(response.result);
      else pending.reject(new Error(`${response.error?.code}: ${response.error?.message}`));
    }
  }

  request(method, params = {}, timeoutMs = 120_000) {
    if (this.child.exitCode !== null) return Promise.reject(new Error("engine is not running"));
    const id = `runtime-${++this.sequence}`;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`${method} timed out`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timeout });
      this.child.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
    });
  }

  async poll(kind, id, timeoutMs) {
    const key = kind === "query" ? "executionId" : "exportId";
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const status = await this.request(`${kind}.status`, { [key]: id });
      if (terminal.has(status.state)) return status;
      await delay(25);
    }
    throw new Error(`${kind} ${id} exceeded ${timeoutMs} ms`);
  }

  closeInput() {
    if (!this.child.stdin.destroyed) this.child.stdin.end();
  }

  rejectAll(error) {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(error);
    }
    this.pending.clear();
  }
}

async function waitUntilRunning(client, exportId, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const status = await client.request("export.status", { exportId });
    if (status.state === "running") return;
    if (terminal.has(status.state))
      throw new Error(`long export became ${status.state} before cancellation`);
    await delay(10);
  }
  throw new Error("long export did not enter running state");
}

async function processSnapshot() {
  const script = [
    "Get-CimInstance Win32_Process |",
    "Select-Object @{N='pid';E={$_.ProcessId}},@{N='parentPid';E={$_.ParentProcessId}},@{N='name';E={$_.Name}},@{N='executablePath';E={$_.ExecutablePath}},@{N='commandLine';E={$_.CommandLine}},@{N='workingSetBytes';E={[double]$_.WorkingSetSize}} |",
    "ConvertTo-Json -Compress",
  ].join(" ");
  const { stdout } = await execPowerShell(script, 32 * 1024 * 1024);
  if (!stdout.trim()) return [];
  const parsed = JSON.parse(stdout);
  return Array.isArray(parsed) ? parsed : [parsed];
}

async function processWindow(pid) {
  const script = `$p = Get-Process -Id ${Number(pid)} -ErrorAction Stop; [pscustomobject]@{ mainWindowHandle = [int64]$p.MainWindowHandle; responding = [bool]$p.Responding } | ConvertTo-Json -Compress`;
  try {
    return JSON.parse((await execPowerShell(script)).stdout);
  } catch {
    return null;
  }
}

async function closeMainWindow(pid) {
  const script = `$p = Get-Process -Id ${Number(pid)} -ErrorAction Stop; if (-not $p.CloseMainWindow()) { throw 'Tarik main window did not accept close' }`;
  await execPowerShell(script);
  return true;
}

async function terminateTree(pid) {
  await execFileAsync("taskkill.exe", ["/PID", String(Number(pid)), "/T", "/F"]);
}

async function processWorkingSet(pid) {
  const { stdout } = await execPowerShell(
    `(Get-Process -Id ${Number(pid)} -ErrorAction Stop).WorkingSet64`,
  );
  const value = Number(stdout.trim());
  if (!Number.isFinite(value) || value < 0) throw new Error("invalid process working set");
  return value;
}

async function executableVersion(executable) {
  if (!executable) return null;
  try {
    const { stdout } = await execPowerShell(
      `(Get-Item -LiteralPath '${quotePowerShell(executable)}').VersionInfo.ProductVersion`,
    );
    return stdout.trim() || null;
  } catch {
    return null;
  }
}

function execPowerShell(script, maxBuffer = 4 * 1024 * 1024) {
  return execFileAsync(
    "powershell.exe",
    ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
    { cwd: root, encoding: "utf8", maxBuffer },
  );
}

function quotePowerShell(value) {
  return value.replaceAll("'", "''");
}

function processExit(child) {
  return new Promise((resolve) => {
    child.once("exit", (code, signal) => resolve({ code, signal }));
  });
}

async function fileExists(file) {
  try {
    return (await stat(file)).isFile();
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

async function pathExists(candidate) {
  try {
    await stat(candidate);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

async function readOptionalText(file) {
  try {
    return await readFile(file, "utf8");
  } catch (error) {
    if (error?.code === "ENOENT") return "";
    throw error;
  }
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

await main();
