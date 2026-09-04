import assert from "node:assert/strict";
import test from "node:test";
import {
  cargoProfileArguments,
  engineExecutableName,
  isProjectDevProcess,
  normalizePath,
  profileDirectory,
  runtimeLibraryName,
  rustHostTriple,
} from "../scripts/platform-tools.mjs";
import { appDataRoot } from "../scripts/reset-app-data.mjs";

test("platform artifact names and Cargo profiles are explicit", () => {
  assert.equal(engineExecutableName("win32"), "tarik-engine-duckdb.exe");
  assert.equal(engineExecutableName("linux"), "tarik-engine-duckdb");
  assert.equal(runtimeLibraryName("win32"), "duckdb.dll");
  assert.equal(runtimeLibraryName("linux"), "libduckdb.so");
  assert.equal(profileDirectory("debug"), "debug");
  assert.equal(profileDirectory("release"), "release");
  assert.deepEqual(cargoProfileArguments("debug"), []);
  assert.deepEqual(cargoProfileArguments("release"), ["--release"]);
  assert.deepEqual(cargoProfileArguments("dev-fast"), ["--profile", "dev-fast"]);
  assert.equal(
    rustHostTriple("rustc 1.91.0\nhost: x86_64-pc-windows-msvc\nrelease: 1.91.0\n"),
    "x86_64-pc-windows-msvc",
  );
});

test("Windows paths normalize drive case and separators", () => {
  assert.equal(
    normalizePath("C:\\Users\\Data Engineer\\Tarik", "win32"),
    "c:/users/data engineer/tarik",
  );
  assert.equal(
    normalizePath("c:/USERS/Data Engineer/Tarik/", "win32"),
    "c:/users/data engineer/tarik",
  );
});

test("development reset matches only Tarik processes rooted in this checkout", () => {
  const root = "C:\\Users\\Data Engineer\\Tarik";
  assert.equal(
    isProjectDevProcess(
      {
        pid: 7,
        executablePath: "C:\\Users\\Data Engineer\\Tarik\\target\\debug\\tarik.exe",
        commandLine: '"C:\\Users\\Data Engineer\\Tarik\\target\\debug\\tarik.exe"',
      },
      root,
      "win32",
    ),
    true,
  );
  assert.equal(
    isProjectDevProcess(
      {
        pid: 8,
        executablePath: "C:\\Other\\Tarik\\target\\debug\\tarik.exe",
        commandLine: '"C:\\Other\\Tarik\\target\\debug\\tarik.exe"',
      },
      root,
      "win32",
    ),
    false,
  );
  assert.equal(
    isProjectDevProcess(
      { pid: 9, executablePath: "C:\\Windows\\System32\\node.exe", commandLine: "node server.js" },
      root,
      "win32",
    ),
    false,
  );
});

test("app-data reset resolves native Windows and XDG locations", () => {
  assert.equal(
    appDataRoot("win32", { APPDATA: "C:\\Users\\Ada\\AppData\\Roaming" }, "unused"),
    "C:\\Users\\Ada\\AppData\\Roaming\\com.tarik.desktop",
  );
  assert.equal(
    appDataRoot("linux", { XDG_DATA_HOME: "/tmp/xdg data" }, "/home/ada"),
    "/tmp/xdg data/com.tarik.desktop",
  );
  assert.equal(appDataRoot("linux", {}, "/home/ada"), "/home/ada/.local/share/com.tarik.desktop");
});
