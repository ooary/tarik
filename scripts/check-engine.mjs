#!/usr/bin/env node

import { spawn } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { engineExecutableName } from "./platform-tools.mjs";

const root = path.resolve(import.meta.dirname, "..");
const engine = path.join(root, "target", "debug", engineExecutableName());

try {
  const response = await handshake(engine);
  if (!response.ok) {
    throw new Error(response.error?.message || "engine returned an empty failure");
  }
  if (response.result?.protocolVersion !== 1) {
    throw new Error(`expected protocol 1, found ${response.result?.protocolVersion}`);
  }
  console.log(
    `Engine OK: ${response.result.engineId} ${response.result.engineVersion} protocol ${response.result.protocolVersion}`,
  );
  console.log("Ready. Run the app with: npm run tauri dev");
} catch (error) {
  console.error(`Engine check failed for ${engine}: ${error.message}`);
  console.error("Build it with: npm run engine:build");
  process.exitCode = 1;
}

function handshake(executable) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, [], { cwd: path.dirname(executable), shell: false });
    let stdout = "";
    let stderr = "";
    let settled = false;
    const timeout = setTimeout(() => finish(new Error("handshake timed out")), 10_000);

    function finish(error, value) {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      child.kill();
      if (error) reject(error);
      else resolve(value);
    }

    child.once("error", finish);
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      if (stderr.length > 16_384) stderr = stderr.slice(-16_384);
    });
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      const newline = stdout.indexOf("\n");
      if (newline < 0) return;
      try {
        finish(null, JSON.parse(stdout.slice(0, newline)));
      } catch (error) {
        finish(new Error(`invalid JSON response: ${error.message}`));
      }
    });
    child.once("exit", (code) => {
      if (!settled) finish(new Error(`engine exited with ${code}: ${stderr.trim()}`));
    });
    child.stdin.end('{"id":"check","method":"engine.handshake","params":{}}\n');
  });
}
