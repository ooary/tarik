#!/usr/bin/env node

import path from "node:path";
import process from "node:process";
import {
  isProjectDevProcess,
  listProcesses,
  terminateProcessTree,
  waitForPortRelease,
} from "./platform-tools.mjs";

const projectRoot = path.resolve(import.meta.dirname, "..");

try {
  const records = await listProcesses();
  const matches = records.filter(
    (record) => record.pid !== process.pid && isProjectDevProcess(record, projectRoot),
  );
  // Parent termination normally covers its descendants. Try shallowest PIDs
  // first; already-exited children are harmless.
  const ids = [...new Set(matches.map((record) => record.pid))].sort((a, b) => a - b);
  for (const pid of ids) {
    try {
      await terminateProcessTree(pid);
    } catch (error) {
      console.warn(`Could not stop Tarik dev process ${pid}: ${error.message}`);
    }
  }
  if (!(await waitForPortRelease(1420))) {
    throw new Error("Tarik dev port 1420 is still occupied by another process");
  }
  console.log(
    ids.length > 0 ? `Stopped ${ids.length} Tarik dev process(es).` : "Tarik dev state is clear.",
  );
} catch (error) {
  console.error(`Development reset failed: ${error.message}`);
  process.exitCode = 1;
}
