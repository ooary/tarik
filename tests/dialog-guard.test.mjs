import assert from "node:assert/strict";
import test from "node:test";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const browserDialogPattern = /\bwindow\s*\.\s*(?:alert|prompt|confirm)\s*\(/g;

async function sourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await sourceFiles(target)));
    else if (/\.[cm]?[jt]sx?$/.test(entry.name) && !entry.name.includes(".test."))
      files.push(target);
  }
  return files;
}

test("production frontend does not use browser alert, prompt, or confirm", async () => {
  const files = await sourceFiles(fileURLToPath(new URL("../src", import.meta.url)));
  const violations = [];
  for (const file of files) {
    const source = await readFile(file, "utf8");
    for (const match of source.matchAll(browserDialogPattern)) {
      const line = source.slice(0, match.index).split("\n").length;
      violations.push(`${path.relative(process.cwd(), file)}:${line}: ${match[0]}`);
    }
  }
  assert.deepEqual(violations, []);
});

test("native operating-system file and folder pickers remain wired", async () => {
  const commands = await readFile(new URL("../src/lib/commands.ts", import.meta.url), "utf8");
  assert.match(commands, /@tauri-apps\/plugin-dialog/);
  assert.match(commands, /function chooseSourceFile/);
  assert.match(commands, /function chooseDuckDbFile/);
  assert.match(commands, /function chooseExportDirectory/);
});
