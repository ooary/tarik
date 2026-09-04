#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET_TRIPLE="${TARGET_TRIPLE:-$(rustc -vV | awk '/^host:/{print $2}')}"
if [[ "$TARGET_TRIPLE" != "x86_64-unknown-linux-gnu" ]]; then
  printf 'E11 Linux release currently supports x86_64-unknown-linux-gnu, found %s\n' "$TARGET_TRIPLE" >&2
  exit 1
fi

VERSION=$(node -p 'require("./package.json").version')
TAURI_VERSION=$(node -p 'require("./src-tauri/tauri.conf.json").version')
CARGO_VERSION=$(awk '
  /^\[package\]/{package=1; next}
  package && /^version = /{gsub(/[" ]/, "", $3); print $3; exit}
' src-tauri/Cargo.toml)
if [[ "$VERSION" != "$TAURI_VERSION" || "$VERSION" != "$CARGO_VERSION" ]]; then
  printf 'Version mismatch: npm=%s tauri=%s cargo=%s\n' "$VERSION" "$TAURI_VERSION" "$CARGO_VERSION" >&2
  exit 1
fi

STAGE="target/release-artifacts"
PORTABLE="$STAGE/Tarik-$VERSION-linux-x86_64"
ARCHIVE="$STAGE/Tarik-$VERSION-linux-x86_64.tar.gz"
CHECKSUMS="$STAGE/SHA256SUMS"
rm -rf "$STAGE"
mkdir -p "$STAGE" "$PORTABLE"

printf '==> Build and stage DuckDB sidecar\n'
CARGO_BUILD_PROFILE=release ./scripts/build-engine.sh
cp target/release/tarik-engine-duckdb "src-tauri/binaries/tarik-engine-duckdb-$TARGET_TRIPLE"
cp target/release/libduckdb.so "src-tauri/binaries/libduckdb.so-$TARGET_TRIPLE"
trap 'rm -f "src-tauri/binaries/tarik-engine-duckdb-$TARGET_TRIPLE" "src-tauri/binaries/libduckdb.so-$TARGET_TRIPLE"' EXIT

printf '==> Verify sidecar runtime link\n'
readelf -d target/release/tarik-engine-duckdb | grep -Eq '\((RPATH|RUNPATH)\).*\$ORIGIN'
ldd target/release/tarik-engine-duckdb | grep -Eq 'libduckdb\.so => .*/target/release/libduckdb\.so'

printf '==> Build Tauri Linux bundles\n'
# Arch and other rolling distributions can expose symbols unknown to the
# linuxdeploy strip tool. Rust release binaries are already stripped by Cargo.
NO_STRIP=true npm run tauri build -- --config src-tauri/tauri.release.conf.json --bundles deb,appimage

printf '==> Generate dependency inventories\n'
cargo metadata --format-version 1 > "$STAGE/cargo-metadata.json"
python3 - "$STAGE" <<'PY'
import json, pathlib, sys
stage=pathlib.Path(sys.argv[1])
cargo=json.loads((stage/'cargo-metadata.json').read_text())
rows=sorted({(p['name'],p['version'],p.get('license') or 'NOASSERTION',p.get('repository') or '') for p in cargo['packages'] if p['name'] != 'tarik'})
(stage/'THIRD-PARTY-RUST.txt').write_text('\n'.join(f'{n} {v}\t{lic}\t{repo}' for n,v,lic,repo in rows)+'\n')
lock=json.loads(pathlib.Path('package-lock.json').read_text())
rows=[]
for path,p in lock['packages'].items():
    if path and 'node_modules/' in path and p.get('version'):
        rows.append((path.rsplit('node_modules/',1)[-1],p['version'],p.get('license') or 'NOASSERTION'))
(stage/'THIRD-PARTY-NPM.txt').write_text('\n'.join(f'{n} {v}\t{lic}' for n,v,lic in sorted(set(rows)))+'\n')
PY
rm "$STAGE/cargo-metadata.json"

printf '==> Assemble portable archive\n'
cp target/release/tarik "$PORTABLE/"
cp target/release/tarik-engine-duckdb "$PORTABLE/tarik-engine-duckdb"
cp target/release/libduckdb.so "$PORTABLE/libduckdb.so"
cp LICENSE THIRD_PARTY_NOTICES.md README.md "$PORTABLE/"
cp docs/release/COMPATIBILITY.md "$PORTABLE/COMPATIBILITY.md"
cp "$STAGE/THIRD-PARTY-RUST.txt" "$STAGE/THIRD-PARTY-NPM.txt" "$PORTABLE/"
tar -C "$STAGE" -czf "$ARCHIVE" "$(basename "$PORTABLE")"

printf '==> Verify staged sidecar handshake\n'
printf '%s\n' '{"id":"release","method":"engine.handshake","params":{}}' \
  | "$PORTABLE/tarik-engine-duckdb" \
  | python3 -c 'import json,sys; d=json.loads(sys.stdin.readline()); assert d["ok"] and d["result"]["protocolVersion"] == 1; print("Release sidecar OK:", d["result"]["engineId"], d["result"]["engineVersion"])'

printf '==> Smoke desktop under clean XDG directories\n'
SMOKE_ROOT=$(mktemp -d -t tarik-release-smoke-XXXXXX)
set +e
XDG_DATA_HOME="$SMOKE_ROOT/data" XDG_CACHE_HOME="$SMOKE_ROOT/cache" XDG_CONFIG_HOME="$SMOKE_ROOT/config" \
  WEBKIT_DISABLE_DMABUF_RENDERER=1 timeout 8s "$PORTABLE/tarik" >"$SMOKE_ROOT/stdout" 2>"$SMOKE_ROOT/stderr"
SMOKE_RC=$?
set -e
if [[ $SMOKE_RC -ne 0 && $SMOKE_RC -ne 124 ]]; then
  printf 'Desktop smoke failed (%s):\n' "$SMOKE_RC" >&2
  tail -100 "$SMOKE_ROOT/stderr" >&2
  rm -rf "$SMOKE_ROOT"
  exit 1
fi
if grep -Eqi 'panic|could not start engine|protocol mismatch|error while running Tarik' "$SMOKE_ROOT/stderr"; then
  cat "$SMOKE_ROOT/stderr" >&2
  rm -rf "$SMOKE_ROOT"
  exit 1
fi
rm -rf "$SMOKE_ROOT"

printf '==> Checksums and manifest\n'
# Tauri artifacts live below target/release/bundle; copy stable release files to one publication directory.
find target/release/bundle -type f \( -name '*.deb' -o -name '*.AppImage' \) -print0 \
  | while IFS= read -r -d '' artifact; do cp "$artifact" "$STAGE/"; done

printf '==> Verify bundle contents and AppImage startup\n'
DEB=$(find "$STAGE" -maxdepth 1 -name '*.deb' -print -quit)
APPIMAGE=$(find "$STAGE" -maxdepth 1 -name '*.AppImage' -print -quit)
DEB_CONTENTS="$STAGE/.deb-contents"
ar p "$DEB" data.tar.gz | tar -tzf - > "$DEB_CONTENTS"
for expected in usr/bin/tarik usr/bin/tarik-engine-duckdb usr/bin/libduckdb.so usr/lib/Tarik/COMPATIBILITY.md usr/lib/Tarik/THIRD_PARTY_NOTICES.md; do
  grep -Fx "$expected" "$DEB_CONTENTS" >/dev/null
done
rm "$DEB_CONTENTS"
SMOKE_ROOT=$(mktemp -d -t tarik-appimage-smoke-XXXXXX)
set +e
XDG_DATA_HOME="$SMOKE_ROOT/data" XDG_CACHE_HOME="$SMOKE_ROOT/cache" XDG_CONFIG_HOME="$SMOKE_ROOT/config" \
  WEBKIT_DISABLE_DMABUF_RENDERER=1 APPIMAGE_EXTRACT_AND_RUN=1 timeout 8s "$APPIMAGE" >"$SMOKE_ROOT/stdout" 2>"$SMOKE_ROOT/stderr"
SMOKE_RC=$?
set -e
if [[ $SMOKE_RC -ne 0 && $SMOKE_RC -ne 124 ]] || grep -Eqi 'panic|could not start engine|protocol mismatch|error while running Tarik' "$SMOKE_ROOT/stderr"; then
  cat "$SMOKE_ROOT/stderr" >&2
  rm -rf "$SMOKE_ROOT"
  exit 1
fi
rm -rf "$SMOKE_ROOT"

: > "$CHECKSUMS"
while IFS= read -r artifact; do
  (cd "$STAGE" && sha256sum "$(basename "$artifact")") >> "$CHECKSUMS"
done < <(find "$STAGE" -maxdepth 1 -type f \( -name '*.deb' -o -name '*.AppImage' -o -name '*.tar.gz' \) -print | sort)
(cd "$STAGE" && sha256sum -c SHA256SUMS)

python3 - "$STAGE" "$VERSION" "$TARGET_TRIPLE" <<'PY'
import hashlib,json,pathlib,subprocess,sys
stage=pathlib.Path(sys.argv[1]); version=sys.argv[2]; target=sys.argv[3]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
artifacts=[]
for path in sorted(stage.iterdir()):
    if path.is_file() and (path.suffix in {'.deb','.AppImage','.gz'}):
        artifacts.append({'file':path.name,'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
manifest={
  'schemaVersion':1,
  'product':'Tarik',
  'version':version,
  'target':target,
  'gitRevision':revision,
  'signed':False,
  'checksums':'SHA256SUMS',
  'artifacts':artifacts,
  'compatibility':{'metadataSchemaVersion':7,'engineProtocolVersion':1,'duckdbVersion':'1.5.5'},
  'runtime':{'linux':['WebKitGTK 4.1','GTK 3','glibc-compatible x86_64 userspace']},
  'deferredReleaseGates':['E6 final review','E7 final review','E10 manual review']
}
(stage/'release-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
assert artifacts, 'no release artifacts found'
PY

printf '\nRelease artifacts: %s\n' "$STAGE"
find "$STAGE" -maxdepth 1 -type f -printf '  %f (%s bytes)\n' | sort
