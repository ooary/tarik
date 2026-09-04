#!/usr/bin/env python3
"""Check local Markdown links, documented npm commands, and release-version facts."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MARKDOWN = [ROOT / "README.md", *sorted((ROOT / "docs").rglob("*.md"))]
LINK = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
NPM_COMMAND = re.compile(r"\bnpm run ([a-zA-Z0-9:_-]+)")
errors: list[str] = []

package = json.loads((ROOT / "package.json").read_text())
tauri = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())
cargo = (ROOT / "src-tauri/Cargo.toml").read_text()
cargo_version = re.search(r"(?ms)^\[package\].*?^version = \"([^\"]+)\"", cargo)
versions = {
    "package.json": package["version"],
    "tauri.conf.json": tauri["version"],
    "Cargo.toml": cargo_version.group(1) if cargo_version else "missing",
}
if len(set(versions.values())) != 1:
    errors.append(f"application version mismatch: {versions}")

for document in MARKDOWN:
    text = document.read_text()
    for target in LINK.findall(text):
        target = target.split("#", 1)[0]
        if not target or "://" in target or target.startswith("mailto:"):
            continue
        path = (document.parent / target).resolve()
        if not path.exists():
            errors.append(f"{document.relative_to(ROOT)}: missing link {target}")
    for command in NPM_COMMAND.findall(text):
        if command not in package["scripts"]:
            errors.append(f"{document.relative_to(ROOT)}: missing npm script {command}")

required_user_topics = {
    "CSV vs Parquet": ["CSV", "Parquet"],
    "import vs link": ["Import", "Link"],
    "Estimate vs Actual Flow": ["Estimate", "Actual Flow"],
    "export": ["Rows per part", "Stop without replacing"],
    "history": ["Saved queries", "History"],
    "logs": ["Reveal logs", "Clear cache"],
    "data location": ["~/.local/share/com.tarik.desktop"],
    "limitations": ["Known limitations"],
}
user_guide = (ROOT / "docs/user/USER-GUIDE.md").read_text()
for topic, phrases in required_user_topics.items():
    if any(phrase not in user_guide for phrase in phrases):
        errors.append(f"user guide missing {topic}: {phrases}")

ship = (ROOT / "docs/release/SHIP-CHECKLIST.md").read_text()
for gate in ["E6 final review", "E7 final review", "E10 review"]:
    if f"- [ ] **{gate}" not in ship:
        errors.append(f"ship checklist does not retain unchecked blocker {gate}")

if errors:
    print("Documentation checks failed:")
    for error in errors:
        print(f"- {error}")
    sys.exit(1)
print(f"Documentation checks passed: {len(MARKDOWN)} Markdown files, version {package['version']}")
