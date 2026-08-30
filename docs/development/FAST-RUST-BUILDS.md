# Faster local Rust builds

Tarik keeps stable Rust and LLVM as the required release/CI toolchain. Linux x64 development can use Clang, mold, and the profile settings recommended for a Rust + Tauri client.

## Install mold and sccache

On Arch/Omarchy with root:

```bash
sudo pacman -S clang mold sccache bacon cargo-nextest
```

Without root, install mold and sccache under `~/.local`:

```bash
./scripts/install-fast-rust-tools.sh
```

Ensure `~/.local/bin` is in `PATH`. Clang is required to resolve `-fuse-ld=mold`, so install it with `sudo pacman -S clang` or an equivalent package manager.

## Project-local Cargo configuration

```toml
# .cargo/config.toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

[build]
# No rustc-wrapper: Cargo incremental is the fast warm path and mold
# handles the expensive link step. sccache stays available but unwrapped.
# jobs = 4
```

mold is the largest link-time win. It is resolved by name through `ld.mold` on `PATH`, matching `-fuse-ld=mold`.

sccache is intentionally **not** set as `rustc-wrapper`. Wrapping rustc there invalidates the existing `target/` fingerprint set and hides Cargo incremental reuse. Keep sccache available for optional cross-project population without wrapping the hot loop.

## Cargo profiles

In `src-tauri/Cargo.toml`:

```toml
[profile.dev]
opt-level = 1
debug = "line-tables-only"
split-debuginfo = "unpacked"
incremental = true

[profile.dev.package."*"]
opt-level = 2
debug = false

[profile.release]
strip = true
lto = "thin"
codegen-units = 1

[profile.dev-fast]
inherits = "release"
debug = true
```

Deliberately avoided:

- `opt-level = "z"` in release
- `panic = "abort"`
- `lto = "fat"`

`dev-fast` is available as `cargo run --profile dev-fast` and is not part of normal iteration.

## Measured behavior on this machine

Switching the linker/profile creates a new Cargo fingerprint and caused one full rebuild:

```text
First check after changing dependency opt-level:
~14m26s

Then warm check:
0.68s
Warm clippy:
1.57s
Warm test run:
2s
```

`debug = "line-tables-only"` can reduce `target/` by roughly 40-60%. To remove stale artifacts without a manual `rm -rf`, use:

```bash
cargo sweep --time 30
```

Do not use `cargo clean` as a routine blank-screen fix.

## Daily workflow

```bash
bacon                      # terminal check on save
cargo nextest run           # faster tests as the suite grows
cargo test --doc             # keep doc-test coverage
```

## Blank screen on Tauri dev

Blank/white screen is a WebView or JS error, not a Rust cache problem. In order:

1. Open DevTools and inspect the Console for a React/JS exception.
2. Try `WEBKIT_DISABLE_DMABUF_RENDERER=1 npm run tauri dev`.
3. Reset only the stale WebView cache:
   ```bash
   rm -rf ~/.local/share/<bundle-identifier>
   ```
4. Avoid `cargo clean` as a routine fix.

## Memory measurement

Do not measure app memory from a debug build. Use:

```bash
cargo build --release
/usr/bin/time -v ./target/release/app
```

## Cranelift

Cranelift remains optional and unconfigured. It requires nightly, creates a separate artifact set, and does not speed up DuckDB C++ compilation. Benchmark stable plus mold plus incremental before adopting it. Release and CI builds stay on stable LLVM.
