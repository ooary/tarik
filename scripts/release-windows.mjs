#!/usr/bin/env node

import { execFile, spawn } from "node:child_process";
import { copyFile, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { promisify } from "node:util";
import {
  assertPortableContents,
  assertWindowsReleaseHost,
  portableManifest,
  PORTABLE_FILES,
  sha256,
  verifyPortableChecksums,
  WINDOWS_TARGET,
} from "./windows-package.mjs";
import { findFile, run, rustHostTriple } from "./platform-tools.mjs";

const execFileAsync = promisify(execFile);
const root = path.resolve(import.meta.dirname, "..");
const stage = path.join(root, "target", "release-artifacts", "windows");
const packageJson = JSON.parse(await readFile(path.join(root, "package.json"), "utf8"));
const tauriConfig = JSON.parse(
  await readFile(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"),
);
const version = packageJson.version;
const folderName = `Tarik-${version}-windows-x64-portable`;
const portable = path.join(stage, folderName);
const archiveName = `${folderName}.zip`;
const archive = path.join(stage, archiveName);

try {
  const { stdout: rustVersion } = await execFileAsync("rustc", ["-vV"], { cwd: root });
  assertWindowsReleaseHost({
    platform: process.platform,
    arch: process.arch,
    rustHost: rustHostTriple(rustVersion),
  });
  const cargoManifest = await readFile(path.join(root, "src-tauri", "Cargo.toml"), "utf8");
  const cargoVersion = cargoManifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (version !== tauriConfig.version || version !== cargoVersion) {
    throw new Error(
      `version mismatch: npm=${version}, tauri=${tauriConfig.version}, cargo=${cargoVersion}`,
    );
  }

  await rm(stage, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
  await mkdir(portable, { recursive: true });

  console.log("==> Build pinned DuckDB sidecar");
  await run("cargo", ["build", "-p", "tarik-engine-duckdb", "--release"], { cwd: root });
  const duckdbDll = await findFile(
    path.join(root, "target", "duckdb-download", WINDOWS_TARGET, "1.5.5"),
    "duckdb.dll",
  );
  if (!duckdbDll) throw new Error("pinned DuckDB 1.5.5 duckdb.dll was not downloaded");
  const targetRelease = path.join(root, "target", "release");
  await copyFile(duckdbDll, path.join(targetRelease, "duckdb.dll"));

  console.log("==> Compile Tauri Windows release executable");
  await run(
    process.execPath,
    [path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js"), "build", "--no-bundle"],
    { cwd: root },
  );

  console.log("==> Generate dependency inventories");
  const { stdout: cargoMetadata } = await execFileAsync(
    "cargo",
    ["metadata", "--format-version", "1"],
    { cwd: root, maxBuffer: 64 * 1024 * 1024 },
  );
  const cargo = JSON.parse(cargoMetadata);
  const rustNotices = [
    ...new Set(
      cargo.packages
        .filter((item) => item.name !== "tarik")
        .map(
          (item) =>
            `${item.name}\t${item.version}\t${item.license || "NOASSERTION"}\t${item.repository || ""}`,
        ),
    ),
  ].sort();
  await writeFile(path.join(portable, "THIRD-PARTY-RUST.txt"), `${rustNotices.join("\n")}\n`);

  const lock = JSON.parse(await readFile(path.join(root, "package-lock.json"), "utf8"));
  const npmNotices = [
    ...new Set(
      Object.entries(lock.packages)
        .filter(([name, item]) => name.includes("node_modules/") && item.version)
        .map(
          ([name, item]) =>
            `${name.slice(name.lastIndexOf("node_modules/") + 13)}\t${item.version}\t${item.license || "NOASSERTION"}`,
        ),
    ),
  ].sort();
  await writeFile(path.join(portable, "THIRD-PARTY-NPM.txt"), `${npmNotices.join("\n")}\n`);

  console.log("==> Assemble portable directory");
  const copies = [
    [path.join(targetRelease, "tarik.exe"), "Tarik.exe"],
    [path.join(targetRelease, "tarik-engine-duckdb.exe"), "tarik-engine-duckdb.exe"],
    [duckdbDll, "duckdb.dll"],
    [path.join(root, "docs", "release", "WINDOWS-PORTABLE-README.md"), "README.md"],
    [path.join(root, "docs", "release", "COMPATIBILITY.md"), "COMPATIBILITY.md"],
    [path.join(root, "LICENSE"), "LICENSE"],
    [path.join(root, "THIRD_PARTY_NOTICES.md"), "THIRD_PARTY_NOTICES.md"],
  ];
  for (const [source, destination] of copies) {
    await copyFile(source, path.join(portable, destination));
  }
  const internalChecksums = [];
  for (const name of PORTABLE_FILES.filter((name) => name !== "SHA256SUMS")) {
    internalChecksums.push(`${await sha256(path.join(portable, name))}  ${name}`);
  }
  await writeFile(path.join(portable, "SHA256SUMS"), `${internalChecksums.join("\n")}\n`);
  await assertPortableContents(portable);
  await verifyPortableChecksums(portable);

  console.log("==> Verify bundled engine handshake");
  const handshake = await engineHandshake(path.join(portable, "tarik-engine-duckdb.exe"));
  if (
    !handshake.ok ||
    handshake.result?.engineId !== "duckdb" ||
    handshake.result?.protocolVersion !== 1
  ) {
    throw new Error(`bundled engine handshake failed: ${JSON.stringify(handshake)}`);
  }

  console.log("==> Create and inspect portable ZIP");
  await execFileAsync(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `Compress-Archive -Path '${portable.replaceAll("'", "''")}' -DestinationPath '${archive.replaceAll("'", "''")}' -CompressionLevel Optimal -Force`,
    ],
    { cwd: root },
  );
  const extracted = path.join(stage, ".verify-extracted");
  await execFileAsync(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `Expand-Archive -Path '${archive.replaceAll("'", "''")}' -DestinationPath '${extracted.replaceAll("'", "''")}' -Force`,
    ],
    { cwd: root },
  );
  await assertPortableContents(path.join(extracted, folderName));
  await verifyPortableChecksums(path.join(extracted, folderName));
  const extractedHandshake = await engineHandshake(
    path.join(extracted, folderName, "tarik-engine-duckdb.exe"),
  );
  if (!extractedHandshake.ok || extractedHandshake.result?.protocolVersion !== 1) {
    throw new Error("extracted bundled engine handshake failed");
  }

  const archiveDigest = await sha256(archive);
  const archiveBytes = (await stat(archive)).size;
  const revision = (await execFileAsync("git", ["rev-parse", "HEAD"], { cwd: root })).stdout.trim();
  await writeFile(path.join(stage, "SHA256SUMS"), `${archiveDigest}  ${archiveName}\n`);
  await writeFile(
    path.join(stage, "release-manifest.json"),
    `${JSON.stringify(
      portableManifest({
        version,
        revision,
        archive: archiveName,
        bytes: archiveBytes,
        digest: archiveDigest,
      }),
      null,
      2,
    )}\n`,
  );
  await rm(extracted, { recursive: true, force: true });
  console.log(`Windows portable release: ${archive}`);
} catch (error) {
  console.error(`Windows release failed: ${error.message}`);
  process.exitCode = 1;
}

function engineHandshake(executable) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, [], {
      cwd: path.dirname(executable),
      shell: false,
    });
    let output = "";
    let errors = "";
    const timeout = setTimeout(() => finish(new Error("engine handshake timed out")), 10_000);
    let settled = false;
    function finish(error, value) {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      child.kill();
      if (error) reject(error);
      else resolve(value);
    }
    child.once("error", finish);
    child.stderr.on("data", (chunk) => (errors += chunk));
    child.stdout.on("data", (chunk) => {
      output += chunk;
      const newline = output.indexOf("\n");
      if (newline < 0) return;
      try {
        finish(null, JSON.parse(output.slice(0, newline)));
      } catch (error) {
        finish(new Error(`invalid engine response: ${error.message}`));
      }
    });
    child.once("exit", (code) => {
      if (!settled) finish(new Error(`engine exited with ${code}: ${errors}`));
    });
    child.stdin.end('{"id":"release","method":"engine.handshake","params":{}}\n');
  });
}
