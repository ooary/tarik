#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  printf 'This helper currently supports Linux x86_64 only.\n' >&2
  exit 1
fi

install_from_arch_package() {
  local package="$1"
  local destination="$HOME/.local/opt/$package"
  local url
  url="$(pacman -Sp --print-format '%l' "$package" | tail -n 1)"
  [[ -n "$url" ]] || { printf 'Could not resolve package URL for %s.\n' "$package" >&2; exit 1; }

  local archive
  archive="$(mktemp --suffix=.pkg.tar.zst)"
  curl -fL "$url" -o "$archive"
  rm -rf "$destination"
  mkdir -p "$destination"
  bsdtar -xf "$archive" -C "$destination"
  rm -f "$archive"
  ln -sf "$destination/usr/bin/$package" "$HOME/.local/bin/$package"
}

mkdir -p "$HOME/.local/bin" "$HOME/.local/opt"
command -v clang >/dev/null || {
  printf 'clang is required. Install it with: sudo pacman -S clang\n' >&2
  exit 1
}
command -v mold >/dev/null || install_from_arch_package mold
command -v sccache >/dev/null || install_from_arch_package sccache

# clang must be able to resolve `-fuse-ld=mold` by name. Clang looks up
# `ld.<name>` on PATH, so expose ld.mold when mold is under ~/.local.
if [[ -x "$HOME/.local/bin/mold" && ! -e "$HOME/.local/bin/ld.mold" ]]; then
  ln -s "$HOME/.local/bin/mold" "$HOME/.local/bin/ld.mold"
fi

printf 'Installed development tools:\n'
"$HOME/.local/bin/mold" --version
"$HOME/.local/bin/sccache" --version

printf '\nOptional workflow tools (install with system packages or Cargo when wanted):\n'
printf '  sudo pacman -S bacon cargo-nextest\n'
printf '  # or: cargo install --locked bacon cargo-nextest\n'
