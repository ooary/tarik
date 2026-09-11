import { createHash } from "node:crypto";
import { readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";

export const WINDOWS_TARGET = "x86_64-pc-windows-msvc";

export const PORTABLE_FILES = [
  "Tarik.exe",
  "tarik-engine-duckdb.exe",
  "tarik-mcp.exe",
  "duckdb.dll",
  "README.md",
  "COMPATIBILITY.md",
  "LICENSE",
  "THIRD_PARTY_NOTICES.md",
  "THIRD-PARTY-RUST.txt",
  "THIRD-PARTY-NPM.txt",
  "agent-skills/tarik-mcp/SKILL.md",
  "SHA256SUMS",
];

export function assertWindowsReleaseHost({ platform, arch, rustHost }) {
  if (platform !== "win32") throw new Error("Windows portable releases must be built on Windows");
  if (arch !== "x64") throw new Error(`Windows portable releases require x64 Node, found ${arch}`);
  if (rustHost !== WINDOWS_TARGET) {
    throw new Error(
      `Windows portable releases require Rust host ${WINDOWS_TARGET}, found ${rustHost}`,
    );
  }
}

export async function sha256(file) {
  const hash = createHash("sha256");
  hash.update(await readFile(file));
  return hash.digest("hex");
}

export async function listRelativeFiles(root) {
  const files = [];
  async function visit(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const absolute = path.join(directory, entry.name);
      if (entry.isDirectory()) await visit(absolute);
      else if (entry.isFile()) files.push(path.relative(root, absolute).replaceAll("\\", "/"));
      else throw new Error(`portable package contains a non-file entry: ${absolute}`);
    }
  }
  await visit(root);
  return files.sort();
}

export async function assertPortableContents(root) {
  const observed = await listRelativeFiles(root);
  const expected = [...PORTABLE_FILES].sort();
  if (JSON.stringify(observed) !== JSON.stringify(expected)) {
    throw new Error(
      `portable contents differ\nexpected: ${expected.join(", ")}\nobserved: ${observed.join(", ")}`,
    );
  }
  for (const executable of [
    "Tarik.exe",
    "tarik-engine-duckdb.exe",
    "tarik-mcp.exe",
    "duckdb.dll",
  ]) {
    const file = path.join(root, executable);
    if ((await stat(file)).size < 2) throw new Error(`${executable} is empty`);
    if ((await readFile(file)).subarray(0, 2).toString("ascii") !== "MZ") {
      throw new Error(`${executable} is not a Windows PE file`);
    }
  }
}

export async function verifyPortableChecksums(root) {
  const checksumFile = await readFile(path.join(root, "SHA256SUMS"), "utf8");
  const observed = new Map();
  for (const line of checksumFile.trim().split(/\r?\n/)) {
    const match = line.match(/^([a-f0-9]{64}) {2}([A-Za-z0-9._/-]+)$/);
    if (!match) throw new Error(`invalid portable checksum line: ${line}`);
    const name = match[2];
    const segments = name.split("/");
    if (
      name.startsWith("/") ||
      name.includes("\\") ||
      segments.some((segment) => segment === "" || segment === "." || segment === "..")
    ) {
      throw new Error(`unsafe portable checksum path: ${name}`);
    }
    if (observed.has(name)) throw new Error(`duplicate portable checksum: ${name}`);
    observed.set(name, match[1]);
  }
  const expected = PORTABLE_FILES.filter((name) => name !== "SHA256SUMS").sort();
  if (JSON.stringify([...observed.keys()].sort()) !== JSON.stringify(expected)) {
    throw new Error("portable checksum entries differ from packaged files");
  }
  for (const [name, digest] of observed) {
    if ((await sha256(path.join(root, name))) !== digest) {
      throw new Error(`portable checksum mismatch: ${name}`);
    }
  }
}

export function portableManifest({ version, revision, archive, bytes, digest }) {
  return {
    schemaVersion: 1,
    product: "Tarik",
    version,
    target: WINDOWS_TARGET,
    gitRevision: revision,
    signed: false,
    portable: true,
    checksums: "SHA256SUMS",
    artifacts: [{ file: archive, bytes, sha256: digest }],
    compatibility: {
      metadataSchemaVersion: 10,
      engineProtocolVersion: 1,
      duckdbVersion: "1.5.5",
    },
    runtime: {
      windows: ["Windows 10 or 11 x64", "Microsoft Edge WebView2 Runtime"],
    },
    deferredReleaseGates: [
      "native Windows 10 clean-machine review",
      "native Windows 11 clean-machine review",
      "E6 final review",
      "E7 final review",
      "E10 manual review",
    ],
  };
}
