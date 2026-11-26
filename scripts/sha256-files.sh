#!/bin/sh
# Writes <file>.sha256 next to each file: "<hex>  <basename>", as sha256sum prints it.
# Used by .github/workflows/release-artifacts.yml on Linux (sha256sum) and macOS (shasum).
set -eu
for f in "$@"; do
  dir=$(dirname "$f")
  base=$(basename "$f")
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$dir" && sha256sum "$base" > "${base}.sha256")
  else
    (cd "$dir" && shasum -a 256 "$base" > "${base}.sha256")
  fi
done
