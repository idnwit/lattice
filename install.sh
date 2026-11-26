#!/bin/sh
# Install Lattice from GitHub releases, for the machine this runs on.
#   curl -fsSL https://raw.githubusercontent.com/idnwit/lattice/main/install.sh | sh -s -- worker
#   ... | sh -s -- relay --version dev --bin-dir ~/bin
# Components (default: worker):
#   worker  latticed, latticed-proc and the lattice CLI (Linux, macOS)
#   relay   lattice-relay (Linux, macOS; Docker: ghcr.io/idnwit/lattice-relay)
#   deck    Lattice.app into ~/Applications (macOS on Apple silicon)
#   all     worker and relay, plus deck on macOS
# Options: --version dev|latest|vX.Y.Z (default latest; falls back to dev when no versioned
# release exists), --bin-dir DIR (default ~/.local/bin), --app-dir DIR (default
# ~/Applications), --dry-run (print the plan, no network).
# Env: LATTICE_VERSION, LATTICE_BIN_DIR, LATTICE_APP_DIR (flags win); LATTICE_RELEASES_URL
# (a mirror laid out like GitHub releases).
set -eu

REPO="idnwit/lattice"
BASE="${LATTICE_RELEASES_URL:-https://github.com/${REPO}/releases}"

version="${LATTICE_VERSION:-latest}"
bin_dir="${LATTICE_BIN_DIR:-${HOME:-}/.local/bin}"
app_dir="${LATTICE_APP_DIR:-${HOME:-}/Applications}"
dry_run=0
components=""

die() { echo "install.sh: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    worker|relay|deck|all) components="${components} $1"; shift ;;
    --version) [ $# -ge 2 ] || die "--version needs a value"; version="$2"; shift 2 ;;
    --version=*) version="${1#*=}"; shift ;;
    --bin-dir) [ $# -ge 2 ] || die "--bin-dir needs a value"; bin_dir="$2"; shift 2 ;;
    --bin-dir=*) bin_dir="${1#*=}"; shift ;;
    --app-dir) [ $# -ge 2 ] || die "--app-dir needs a value"; app_dir="$2"; shift 2 ;;
    --app-dir=*) app_dir="${1#*=}"; shift ;;
    --dry-run) dry_run=1; shift ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done
[ -n "$components" ] || components="worker"

case "$version" in
  dev|latest|v[0-9]*) ;;
  [0-9]*) version="v${version}" ;;
  *) die "invalid --version '${version}' (use dev, latest or vX.Y.Z)" ;;
esac
[ -n "$bin_dir" ] || die "empty bin dir (set --bin-dir or HOME)"

os="${LATTICE_TEST_OS:-$(uname -s)}"
arch="${LATTICE_TEST_ARCH:-$(uname -m)}"
# A shell under Rosetta reports x86_64 on Apple silicon: install the native build.
if [ "$os" = Darwin ] && [ "$arch" = x86_64 ] && [ -z "${LATTICE_TEST_ARCH:-}" ] \
  && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = 1 ]; then
  arch=arm64
fi
case "${os}/${arch}" in
  Linux/x86_64|Linux/amd64) target="x86_64-unknown-linux-musl" ;;
  Linux/aarch64|Linux/arm64) target="aarch64-unknown-linux-musl" ;;
  Darwin/x86_64) target="x86_64-apple-darwin" ;;
  Darwin/arm64|Darwin/aarch64) target="aarch64-apple-darwin" ;;
  *) die "unsupported platform ${os}/${arch} (supported: Linux x86_64 and aarch64, macOS x86_64 and arm64); build from source instead" ;;
esac

# Expand `all` and check each component against this platform.
plan=""
for c in $components; do
  case "$c" in
    all)
      plan="${plan} worker relay"
      [ "$target" = aarch64-apple-darwin ] && plan="${plan} deck" ;;
    deck)
      [ "$target" = aarch64-apple-darwin ] || die "deck is built for macOS on Apple silicon only (this is ${os}/${arch})"
      plan="${plan} deck" ;;
    *) plan="${plan} ${c}" ;;
  esac
done

asset_of() { # $1 = component
  case "$1" in
    deck) echo "lattice-deck-${target}.zip" ;;
    *) echo "lattice-$1-${target}.tar.gz" ;;
  esac
}

fetch() { # $1 = url, $2 = output file
  if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then wget -q "$1" -O "$2"
  else die "need curl or wget"; fi
}

# `latest` with no versioned release yet falls back to the rolling dev build.
if [ "$version" = latest ] && [ "$dry_run" = 0 ]; then
  probe=$(mktemp)
  first=$(asset_of "$(echo $plan | cut -d' ' -f1)")
  if ! fetch "${BASE}/latest/download/${first}.sha256" "$probe" 2>/dev/null; then
    echo "install.sh: no versioned release yet, using the rolling dev build" >&2
    version=dev
  fi
  rm -f "$probe"
fi
case "$version" in
  latest) base_url="${BASE}/latest/download" ;;
  *) base_url="${BASE}/download/${version}" ;;
esac

echo "version:    ${version}"
echo "target:     ${target}"
echo "components:${plan}"
echo "bin dir:    ${bin_dir}"
case "$plan" in *deck*) echo "app dir:    ${app_dir}" ;; esac
for c in $plan; do echo "url:        ${base_url}/$(asset_of "$c")"; done
[ "$dry_run" = 1 ] && exit 0

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

download() { # $1 = asset; verifies its sha256
  url="${base_url}/$1"
  fetch "$url" "${tmp}/$1" || die "download failed: ${url}"
  fetch "${url}.sha256" "${tmp}/$1.sha256" || die "download failed: ${url}.sha256"
  expected=$(awk '{print $1}' "${tmp}/$1.sha256")
  if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "${tmp}/$1" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then actual=$(shasum -a 256 "${tmp}/$1" | awk '{print $1}')
  else die "need sha256sum or shasum to verify the download"; fi
  [ -n "$expected" ] && [ "$expected" = "$actual" ] || die "checksum mismatch for $1 (expected ${expected}, got ${actual})"
}

install_bins() { # $1 = asset, rest = binaries in it
  asset="$1"; shift
  download "$asset"
  mkdir -p "${tmp}/x" "$bin_dir"
  tar -xzf "${tmp}/${asset}" -C "${tmp}/x" "$@"
  for b in "$@"; do
    mv -f "${tmp}/x/${b}" "${bin_dir}/${b}"
    chmod 755 "${bin_dir}/${b}"
    echo "installed ${bin_dir}/${b}"
  done
}

for c in $plan; do
  case "$c" in
    worker) install_bins "$(asset_of worker)" latticed latticed-proc lattice ;;
    relay) install_bins "$(asset_of relay)" lattice-relay ;;
    deck)
      asset=$(asset_of deck)
      download "$asset"
      mkdir -p "$app_dir"
      rm -rf "${app_dir}/Lattice.app"
      ditto -x -k "${tmp}/${asset}" "$app_dir"
      echo "installed ${app_dir}/Lattice.app"
      ;;
  esac
done

case "$plan" in
  *worker*|*relay*)
    case ":${PATH}:" in
      *":${bin_dir}:"*) ;;
      *) echo "add it to your PATH:  export PATH=\"${bin_dir}:\$PATH\"" ;;
    esac ;;
esac
case "$plan" in
  *worker*) echo "start the worker: latticed-proc & latticed   (see docs/specs/01-pairing.md to pair a deck)" ;;
esac
