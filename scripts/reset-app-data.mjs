#!/usr/bin/env node

import { rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

export function appDataRoot(platform = process.platform, env = process.env, home = os.homedir()) {
  if (platform === "win32") {
    if (!env.APPDATA) throw new Error("APPDATA is unavailable");
    return path.win32.join(env.APPDATA, "com.tarik.desktop");
  }
  if (platform === "darwin") {
    return path.join(home, "Library", "Application Support", "com.tarik.desktop");
  }
  return path.join(env.XDG_DATA_HOME || path.join(home, ".local", "share"), "com.tarik.desktop");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const target = appDataRoot();
    await rm(target, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
    console.log(`Removed Tarik app data: ${target}`);
  } catch (error) {
    console.error(`App-data reset failed: ${error.message}`);
    process.exitCode = 1;
  }
}
