#!/usr/bin/env node

import { readFile, stat } from "node:fs/promises";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const icons = path.join(root, "src-tauri", "icons");
const required = [
  "TarikLogo-square.png",
  "32x32.png",
  "128x128.png",
  "128x128@2x.png",
  "icon.png",
  "icon.ico",
  "icon.icns",
  "StoreLogo.png",
  "Square44x44Logo.png",
  "Square150x150Logo.png",
  "Square310x310Logo.png",
];

for (const name of required) {
  const info = await stat(path.join(icons, name));
  if (!info.isFile() || info.size === 0) throw new Error(`invalid generated icon: ${name}`);
}

const master = await readFile(path.join(icons, "TarikLogo-square.png"));
if (master.readUInt32BE(16) !== 1024 || master.readUInt32BE(20) !== 1024) {
  throw new Error("Tarik icon master must be 1024x1024");
}
if (master[25] !== 6) throw new Error("Tarik icon master must retain RGBA transparency");

const ico = await readFile(path.join(icons, "icon.ico"));
if (ico.readUInt16LE(0) !== 0 || ico.readUInt16LE(2) !== 1 || ico.readUInt16LE(4) < 4) {
  throw new Error("Windows icon.ico is not a multi-size icon resource");
}
const icns = await readFile(path.join(icons, "icon.icns"));
if (icns.subarray(0, 4).toString("ascii") !== "icns") {
  throw new Error("macOS icon.icns header is invalid");
}

const config = JSON.parse(await readFile(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"));
for (const configured of [
  "icons/32x32.png",
  "icons/128x128.png",
  "icons/128x128@2x.png",
  "icons/icon.icns",
  "icons/icon.ico",
]) {
  if (!config.bundle?.icon?.includes(configured)) {
    throw new Error(`Tauri bundle is missing configured icon ${configured}`);
  }
}

console.log("Tarik desktop icon set verified");
