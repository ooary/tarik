import { execFile, spawn } from "node:child_process";
import { createConnection } from "node:net";
import { readdir, readlink } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export const ENGINE_BASENAME = "tarik-engine-duckdb";
export const DUCKDB_VERSION = "1.5.5";

export function engineExecutableName(platform = process.platform) {
  return platform === "win32" ? `${ENGINE_BASENAME}.exe` : ENGINE_BASENAME;
}

export function runtimeLibraryName(platform = process.platform) {
  if (platform === "win32") return "duckdb.dll";
  if (platform === "darwin") return "libduckdb.dylib";
  return "libduckdb.so";
}

export function profileDirectory(profile) {
  if (!profile || profile === "debug") return "debug";
  return profile;
}

export function rustHostTriple(verboseVersion) {
  const match = verboseVersion.match(/^host:\s*(\S+)$/m);
  if (!match) throw new Error("rustc did not report a host target");
  return match[1];
}

export function cargoProfileArguments(profile) {
  if (!profile || profile === "debug") return [];
  if (profile === "release") return ["--release"];
  return ["--profile", profile];
}

export function normalizePath(value, platform = process.platform) {
  const pathApi = platform === "win32" ? path.win32 : path;
  const resolved = pathApi.resolve(value).replaceAll("\\", "/").replace(/\/$/, "");
  return platform === "win32" ? resolved.toLocaleLowerCase("en-US") : resolved;
}

export function isProjectDevProcess(record, projectRoot, platform = process.platform) {
  const root = normalizePath(projectRoot, platform);
  const command = (record.commandLine ?? "").replaceAll("\\", "/");
  const normalizedCommand = platform === "win32" ? command.toLocaleLowerCase("en-US") : command;
  const executable = record.executablePath ? normalizePath(record.executablePath, platform) : "";
  const cwd = record.cwd ? normalizePath(record.cwd, platform) : "";
  const rooted = cwd === root || normalizedCommand.includes(`${root}/`);
  if (!rooted && !executable.startsWith(`${root}/target/`)) return false;

  const roles = [
    `${root}/node_modules/.bin/tauri`,
    `${root}/node_modules/@tauri-apps/cli`,
    `${root}/node_modules/.bin/vite`,
    `${root}/target/debug/tarik`,
    `${root}/target/debug/tarik.exe`,
    `${root}/target/debug/${ENGINE_BASENAME}`,
    `${root}/target/debug/${ENGINE_BASENAME}.exe`,
  ];
  return roles.some((role) => normalizedCommand.includes(role) || executable === role);
}

async function unixProcesses() {
  const entries = await readdir("/proc", { withFileTypes: true });
  const records = [];
  for (const entry of entries) {
    if (!entry.isDirectory() || !/^\d+$/.test(entry.name)) continue;
    const pid = Number(entry.name);
    try {
      const [commandBytes, executablePath, cwd] = await Promise.all([
        import("node:fs/promises").then(({ readFile }) => readFile(`/proc/${pid}/cmdline`)),
        readlink(`/proc/${pid}/exe`),
        readlink(`/proc/${pid}/cwd`),
      ]);
      records.push({
        pid,
        commandLine: commandBytes.toString("utf8").replaceAll("\0", " ").trim(),
        executablePath,
        cwd,
      });
    } catch {
      // Processes can exit or become inaccessible during enumeration.
    }
  }
  return records;
}

async function windowsProcesses() {
  const script = [
    "Get-CimInstance Win32_Process |",
    "Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine |",
    "ConvertTo-Json -Compress",
  ].join(" ");
  const { stdout } = await execFileAsync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-Command", script],
    { maxBuffer: 16 * 1024 * 1024 },
  );
  if (!stdout.trim()) return [];
  const decoded = JSON.parse(stdout);
  return (Array.isArray(decoded) ? decoded : [decoded]).map((record) => ({
    pid: Number(record.ProcessId),
    parentPid: Number(record.ParentProcessId),
    executablePath: record.ExecutablePath ?? "",
    commandLine: record.CommandLine ?? "",
  }));
}

export async function listProcesses(platform = process.platform) {
  if (platform === "win32") return windowsProcesses();
  if (platform === "linux") return unixProcesses();
  const { stdout } = await execFileAsync("ps", ["-axo", "pid=,ppid=,command="]);
  return stdout
    .split("\n")
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.+)$/))
    .filter(Boolean)
    .map((match) => ({
      pid: Number(match[1]),
      parentPid: Number(match[2]),
      commandLine: match[3],
    }));
}

export async function terminateProcessTree(pid, platform = process.platform) {
  if (platform === "win32") {
    await execFileAsync("taskkill.exe", ["/PID", String(pid), "/T", "/F"]);
    return;
  }
  try {
    process.kill(pid, "SIGTERM");
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
}

export function portIsListening(port, host = "127.0.0.1", timeoutMs = 250) {
  return new Promise((resolve) => {
    const socket = createConnection({ host, port });
    const finish = (listening) => {
      socket.destroy();
      resolve(listening);
    };
    socket.setTimeout(timeoutMs, () => finish(false));
    socket.once("connect", () => finish(true));
    socket.once("error", () => finish(false));
  });
}

export async function waitForPortRelease(port, { attempts = 20, delayMs = 100 } = {}) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    if (!(await portIsListening(port))) return true;
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
  return false;
}

export function run(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: "inherit", shell: false, ...options });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with ${code ?? signal}`));
    });
  });
}

export async function findFile(root, filename) {
  const pending = [root];
  while (pending.length > 0) {
    const directory = pending.shift();
    let entries;
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const candidate = path.join(directory, entry.name);
      if (entry.isFile() && entry.name === filename) return candidate;
      if (entry.isDirectory()) pending.push(candidate);
    }
  }
  return null;
}
