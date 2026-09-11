import assert from "node:assert/strict";
import test from "node:test";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  assertPortableContents,
  assertWindowsReleaseHost,
  portableManifest,
  PORTABLE_FILES,
  sha256,
  verifyPortableChecksums,
} from "../scripts/windows-package.mjs";

test("Windows release host check fails closed off native x64 MSVC", () => {
  assert.throws(
    () =>
      assertWindowsReleaseHost({
        platform: "linux",
        arch: "x64",
        rustHost: "x86_64-pc-windows-msvc",
      }),
    /must be built on Windows/,
  );
  assert.throws(
    () =>
      assertWindowsReleaseHost({
        platform: "win32",
        arch: "arm64",
        rustHost: "aarch64-pc-windows-msvc",
      }),
    /require x64 Node/,
  );
  assert.doesNotThrow(() =>
    assertWindowsReleaseHost({
      platform: "win32",
      arch: "x64",
      rustHost: "x86_64-pc-windows-msvc",
    }),
  );
});

test("portable contents are exact and reject missing or extra files", async () => {
  const root = path.join(
    process.env.RUNNER_TEMP || process.env.TEMP || process.env.TMPDIR || "/tmp",
    `tarik-windows-package-${Date.now()}-${process.pid}`,
  );
  await mkdir(root, { recursive: true });
  try {
    for (const file of PORTABLE_FILES) {
      await mkdir(path.dirname(path.join(root, file)), { recursive: true });
      const bytes = [
        "Tarik.exe",
        "tarik-engine-duckdb.exe",
        "tarik-mcp.exe",
        "duckdb.dll",
      ].includes(file)
        ? `MZ${file}`
        : file;
      await writeFile(path.join(root, file), bytes);
    }
    await assertPortableContents(root);
    await writeFile(path.join(root, "unexpected.txt"), "no");
    await assert.rejects(() => assertPortableContents(root), /portable contents differ/);
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
});

test("portable checksums reject tampering after extraction", async () => {
  const root = path.join(
    process.env.RUNNER_TEMP || process.env.TEMP || process.env.TMPDIR || "/tmp",
    `tarik-windows-checksum-${Date.now()}-${process.pid}`,
  );
  await mkdir(root, { recursive: true });
  try {
    const checked = PORTABLE_FILES.filter((name) => name !== "SHA256SUMS");
    for (const file of checked) {
      await mkdir(path.dirname(path.join(root, file)), { recursive: true });
      await writeFile(path.join(root, file), `MZ${file}`);
    }
    const lines = [];
    for (const file of checked) lines.push(`${await sha256(path.join(root, file))}  ${file}`);
    await writeFile(path.join(root, "SHA256SUMS"), `${lines.join("\n")}\n`);
    await verifyPortableChecksums(root);
    await writeFile(path.join(root, "duckdb.dll"), "MZtampered");
    await assert.rejects(() => verifyPortableChecksums(root), /checksum mismatch: duckdb.dll/);
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
});

test("portable checksums reject unsafe nested paths", async () => {
  const root = path.join(
    process.env.RUNNER_TEMP || process.env.TEMP || process.env.TMPDIR || "/tmp",
    `tarik-windows-unsafe-checksum-${Date.now()}-${process.pid}`,
  );
  await mkdir(root, { recursive: true });
  try {
    await writeFile(path.join(root, "SHA256SUMS"), `${"0".repeat(64)}  ../SKILL.md\n`);
    await assert.rejects(() => verifyPortableChecksums(root), /unsafe portable checksum path/);
    await writeFile(path.join(root, "SHA256SUMS"), `${"0".repeat(64)}  agent-skills\\SKILL.md\n`);
    await assert.rejects(() => verifyPortableChecksums(root), /invalid portable checksum line/);
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
});

test("portable manifest records unsigned offline runtime contract", () => {
  const manifest = portableManifest({
    version: "0.1.0",
    revision: "abc123",
    sourceDirty: true,
    archive: "Tarik-0.1.0-windows-x64-portable.zip",
    bytes: 42,
    digest: "deadbeef",
  });
  assert.equal(manifest.target, "x86_64-pc-windows-msvc");
  assert.equal(manifest.sourceDirty, true);
  assert.equal(manifest.signed, false);
  assert.equal(manifest.portable, true);
  assert.equal(manifest.compatibility.duckdbVersion, "1.5.5");
  assert.deepEqual(manifest.runtime.windows, [
    "Windows 10 or 11 x64",
    "Microsoft Edge WebView2 Runtime",
  ]);
  const clean = portableManifest({
    version: "0.1.0",
    revision: "def456",
    sourceDirty: false,
    archive: "Tarik-clean.zip",
    bytes: 7,
    digest: "feedface",
  });
  assert.equal(clean.sourceDirty, false);
});

test("Windows release rebuilds Tarik after verifying icon inputs", async () => {
  const script = await readFile(
    path.resolve(import.meta.dirname, "../scripts/release-windows.mjs"),
    "utf8",
  );
  const verifyIcons = script.indexOf('"verify-icons.mjs"');
  const cleanTarik = script.indexOf('["clean", "-p", "tarik", "--release"]');
  const tauriBuild = script.indexOf('"build", "--no-bundle"');

  assert.ok(verifyIcons >= 0);
  assert.ok(cleanTarik > verifyIcons);
  assert.ok(tauriBuild > cleanTarik);
});
