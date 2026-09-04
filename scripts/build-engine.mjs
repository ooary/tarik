#!/usr/bin/env node

import { execFile } from "node:child_process";
import { copyFile, mkdir } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { promisify } from "node:util";
import {
  cargoProfileArguments,
  engineExecutableName,
  findFile,
  profileDirectory,
  run,
  runtimeLibraryName,
  rustHostTriple,
} from "./platform-tools.mjs";

const execFileAsync = promisify(execFile);
const root = path.resolve(import.meta.dirname, "..");
const profile = process.env.CARGO_BUILD_PROFILE || "debug";
const outputProfile = profileDirectory(profile);
const cargoArgs = ["build", "-p", "tarik-engine-duckdb", ...cargoProfileArguments(profile)];

try {
  await run("cargo", cargoArgs, { cwd: root });
  const outputDirectory = path.join(root, "target", outputProfile);
  const runtimeName = runtimeLibraryName();
  const { stdout: rustVersion } = await execFileAsync("rustc", ["-vV"], { cwd: root });
  const host = rustHostTriple(rustVersion);
  const runtime = await findFile(
    path.join(root, "target", "duckdb-download", host, "1.5.5"),
    runtimeName,
  );
  if (!runtime) {
    throw new Error(
      `${runtimeName} was not found under target/duckdb-download; ensure DUCKDB_DOWNLOAD_LIB=1`,
    );
  }
  await mkdir(outputDirectory, { recursive: true });
  await copyFile(runtime, path.join(outputDirectory, runtimeName));
  console.log(`Copied ${runtimeName} -> ${path.relative(root, outputDirectory)}`);
  console.log(`Engine built: ${path.join("target", outputProfile, engineExecutableName())}`);
  console.log("Run the desktop with: npm run tauri dev");
} catch (error) {
  console.error(`Engine build failed: ${error.message}`);
  process.exitCode = 1;
}
