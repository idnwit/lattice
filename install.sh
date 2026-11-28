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
# ~/Applications), --dry-run (print the plan, the relay, unit files and commands; no network,
# no writes, no questions), --relay URL|none (the worker's home relay; asked on the tty
# otherwise), --no-service (do not install the worker services).
# Env: LATTICE_VERSION, LATTICE_BIN_DIR, LATTICE_APP_DIR, LATTICE_RELAY (flags win);
# LATTICE_RELEASES_URL (a mirror laid out like GitHub releases).
# Worker: the relay goes into config.toml through `lattice config set relay.addr` (an existing
# config.toml is never changed), then latticed-proc and latticed run as user services:
# systemd user units lattice-proc.service and lattice-daemon.service on Linux, LaunchAgents
# dev.lattice.proc and dev.lattice.daemon on macOS (docs/specs/10-settings.md, Install).
set -eu

REPO="idnwit/lattice"
BASE="${LATTICE_RELEASES_URL:-https://github.com/${REPO}/releases}"

version="${LATTICE_VERSION:-latest}"
bin_dir="${LATTICE_BIN_DIR:-${HOME:-}/.local/bin}"
app_dir="${LATTICE_APP_DIR:-${HOME:-}/Applications}"
dry_run=0
no_service=0
relay="${LATTICE_RELAY:-}"
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
    --relay) [ $# -ge 2 ] || die "--relay needs a value"; relay="$2"; shift 2 ;;
    --relay=*) relay="${1#*=}"; shift ;;
    --no-service) no_service=1; shift ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
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
# Service units name binaries by absolute path.
case "$bin_dir" in /*) ;; *) bin_dir="$(pwd)/${bin_dir}" ;; esac

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

warn() { echo "install.sh: warning: $*" >&2; }

# Prints a command; runs it unless --dry-run.
run() {
  echo "+ $*"
  [ "$dry_run" = 1 ] || "$@"
}

# --- Worker relay (docs/specs/10-settings.md, Install rules 47-53) ---

# The data dir, resolved as latticed resolves it (docs/specs/01-pairing.md, Files).
data_dir() {
  if [ -n "${LATTICE_DATA_DIR:-}" ]; then echo "$LATTICE_DATA_DIR"
  elif [ "$os" = Darwin ]; then echo "${HOME}/Library/Application Support/lattice"
  elif [ -n "${XDG_DATA_HOME:-}" ]; then echo "${XDG_DATA_HOME}/lattice"
  else echo "${HOME}/.local/share/lattice"; fi
}

config_file() {
  if [ -n "${LATTICE_CONFIG:-}" ]; then echo "$LATTICE_CONFIG"
  else echo "$(data_dir)/worker/config.toml"; fi
}

# The tty answers: stdin is the curl pipe, so questions go to /dev/tty.
have_tty() { (exec </dev/tty) 2>/dev/null; }

# Asks for the relay on the tty; prints the answer (default: none).
ask_relay() {
  {
    echo "Relay for this worker:"
    echo "  [1] none (direct only, default)"
    echo "  [2] self-hosted: udp://host:port"
    printf "Choice [1]: "
  } >/dev/tty
  read -r choice </dev/tty || choice=""
  case "$choice" in
    ""|1|none) echo none ;;
    2)
      printf "Relay URL (udp://[<relay id>@]host:port): " >/dev/tty
      read -r url </dev/tty || url=""
      echo "$url" ;;
    *) echo "$choice" ;;
  esac
}

# Records the relay with `lattice config set relay.addr`: it validates the URL, and it uses
# the running latticed or takes the worker lock (rule 34). Never touches an existing file.
configure_relay() {
  file=$(config_file)
  if [ -e "$file" ]; then
    current=$( (unset LATTICE_RELAY; "${bin_dir}/lattice" config get relay.addr 2>/dev/null | tail -n 1) || true)
    echo "relay:      ${current:-unknown} (kept: ${file} exists)"
    [ -z "$relay" ] || warn "${file} exists and was left unchanged; --relay/LATTICE_RELAY ignored. Use: lattice config set relay.addr ${relay}"
    return 0
  fi
  answer="$relay"
  asked=0
  if [ -z "$answer" ] && have_tty; then answer=$(ask_relay); asked=1; fi
  [ -n "$answer" ] || answer=none
  # The data dir as latticed makes it: private to this user.
  (umask 077 && mkdir -p "$(data_dir)")
  while :; do
    if out=$( (unset LATTICE_RELAY; "${bin_dir}/lattice" config set relay.addr "$answer") 2>&1); then
      echo "$out"
      return 0
    fi
    case "$out" in
      *config.invalid_value*) ;;
      *) die "recording the relay failed: ${out}" ;;
    esac
    # An invalid URL asks again on a tty (rule 50); without one it fails.
    [ "$asked" = 1 ] || die "config.invalid_value: relay '${answer}' is not none or udp://[<relay id>@]host:port with a canonical host"
    echo "config.invalid_value: '${answer}' is not none or udp://[<relay id>@]host:port with a canonical host" >/dev/tty
    answer=$(ask_relay)
    [ -n "$answer" ] || answer=none
  done
}

# --- Worker services (docs/specs/10-settings.md, User services rules 54-58) ---
# Two units: a latticed restart must never take latticed-proc and its PTYs with it.
# TBD: unit and agent names, file and log locations (docs/specs/10-settings.md, Install).
PROC_UNIT="lattice-proc.service"
DAEMON_UNIT="lattice-daemon.service"
PROC_LABEL="dev.lattice.proc"
DAEMON_LABEL="dev.lattice.daemon"
systemd_dir="${XDG_CONFIG_HOME:-${HOME:-}/.config}/systemd/user"
agents_dir="${HOME:-}/Library/LaunchAgents"
logs_dir="${HOME:-}/Library/Logs/Lattice"

# Writes stdin to $1 through a temporary file; prints it instead under --dry-run.
write_file() {
  if [ "$dry_run" = 1 ]; then
    echo "--- $1"
    cat
    return 0
  fi
  mkdir -p "$(dirname "$1")"
  cat > "$1.tmp"
  mv -f "$1.tmp" "$1"
  echo "wrote $1"
}

# A path as one systemd command-line word: quoted, `%` escaped.
sd_word() { printf '"%s"' "$(printf '%s' "$1" | sed 's/%/%%/g')"; }
# A string as plist text.
xml_text() { printf '%s' "$1" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'; }

# $1 = description, $2 = binary, $3 = extra [Unit] lines, $4 = extra [Service] lines.
# Rule 55: restart on failure with backoff; stop after 5 failures in 60 s (proposed).
systemd_unit() {
  cat <<UNIT
[Unit]
Description=$1
Documentation=https://github.com/idnwit/lattice/blob/main/docs/specs/10-settings.md
StartLimitIntervalSec=60
StartLimitBurst=5$3

[Service]
ExecStart=$(sd_word "${bin_dir}/$2")
Restart=on-failure
RestartSec=5$4

[Install]
WantedBy=default.target
UNIT
}

# $1 = label, $2 = binary, $3 = log file name, $4 = extra keys (trusted plist XML).
launchd_plist() {
  cat <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$1</string>
  <key>ProgramArguments</key>
  <array>
    <string>$(xml_text "${bin_dir}/$2")</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>TMPDIR</key>
    <string>$(xml_text "$mac_tmpdir")</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ThrottleInterval</key>
  <integer>5</integer>$4
  <key>StandardOutPath</key>
  <string>$(xml_text "${logs_dir}/$3")</string>
  <key>StandardErrorPath</key>
  <string>$(xml_text "${logs_dir}/$3")</string>
</dict>
</plist>
PLIST
}

# The user manager answers: systemd runs and this session reaches its user bus.
have_user_systemd() {
  command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1
}

manual_start() { echo "start the worker by hand: latticed-proc & latticed"; }

systemd_install() {
  if [ "$dry_run" = 0 ] && ! have_user_systemd; then
    # Rule 58: no service manager: continue as --no-service.
    warn "systemctl --user is not available (no systemd, a container, or WSL without systemd): worker services not installed"
    manual_start
    return 0
  fi
  systemd_unit "Lattice process daemon (latticed-proc)" latticed-proc "" "
# Stop and restart kill only the daemon, never the processes it spawned.
KillMode=process" | write_file "${systemd_dir}/${PROC_UNIT}"
  # Rule 55: latticed-proc starts first.
  systemd_unit "Lattice worker daemon (latticed)" latticed "
Wants=${PROC_UNIT}
After=${PROC_UNIT}" "" | write_file "${systemd_dir}/${DAEMON_UNIT}"
  proc_was_active=0
  if [ "$dry_run" = 0 ] && systemctl --user is-active --quiet "$PROC_UNIT"; then proc_was_active=1; fi
  run systemctl --user daemon-reload || { warn "systemctl --user daemon-reload failed"; return 0; }
  run systemctl --user enable "$PROC_UNIT" "$DAEMON_UNIT" || { warn "enabling the worker services failed"; return 0; }
  # Rule 57: latticed-proc keeps running (start is a no-op then); only latticed restarts.
  run systemctl --user start "$PROC_UNIT" || warn "starting ${PROC_UNIT} failed (journalctl --user -u ${PROC_UNIT})"
  run systemctl --user restart "$DAEMON_UNIT" || warn "starting ${DAEMON_UNIT} failed (journalctl --user -u ${DAEMON_UNIT})"
  if [ "$proc_was_active" = 1 ]; then
    echo "latticed-proc kept running: the new binary takes over at its next start. Restarting it now ends running sessions: systemctl --user restart ${PROC_UNIT}"
  fi
  user="${USER:-$(id -un)}"
  # TBD: start at boot without a login (linger)? The script only prints the hint.
  if [ "$dry_run" = 1 ]; then
    echo "# if linger is off: loginctl enable-linger \"${user}\""
  elif [ "$(loginctl show-user "$user" --property=Linger --value 2>/dev/null || true)" != yes ]; then
    echo "linger is off: the worker stops when you log out. To keep it running: loginctl enable-linger \"${user}\""
  fi
}

# Waits up to 10 s until launchd no longer knows $1 (bootout returns before the job is gone).
launchd_wait_gone() {
  n=0
  while launchctl print "gui/${uid}/$1" >/dev/null 2>&1 && [ "$n" -lt 10 ]; do
    sleep 1
    n=$((n + 1))
  done
}

launchd_install() {
  uid=$(id -u)
  # The CLI finds the runtime dir through TMPDIR: give the agents the login session's value.
  mac_tmpdir=$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null || true)
  [ -n "$mac_tmpdir" ] || mac_tmpdir="${TMPDIR:-/tmp}"
  proc_plist="${agents_dir}/${PROC_LABEL}.plist"
  daemon_plist="${agents_dir}/${DAEMON_LABEL}.plist"
  run mkdir -p "$logs_dir"
  launchd_plist "$PROC_LABEL" latticed-proc proc.log "
  <key>AbandonProcessGroup</key>
  <true/>" | write_file "$proc_plist"
  launchd_plist "$DAEMON_LABEL" latticed daemon.log "" | write_file "$daemon_plist"
  # Rules 55 and 57: latticed-proc first; never reload a running one (that ends sessions).
  if [ "$dry_run" = 1 ]; then
    echo "# unless ${PROC_LABEL} is loaded already:"
    run launchctl enable "gui/${uid}/${PROC_LABEL}"
    run launchctl bootstrap "gui/${uid}" "$proc_plist"
  elif launchctl print "gui/${uid}/${PROC_LABEL}" >/dev/null 2>&1; then
    echo "latticed-proc kept running: the new binary and plist take over at next login. Reloading it now ends running sessions: launchctl bootout gui/${uid}/${PROC_LABEL}, then rerun this script"
  else
    run launchctl enable "gui/${uid}/${PROC_LABEL}" || true
    if ! run launchctl bootstrap "gui/${uid}" "$proc_plist"; then
      # Rule 58: no GUI login session (SSH only): continue as --no-service.
      warn "loading ${PROC_LABEL} failed (no GUI login session? log in at the console, then rerun)"
      manual_start
      return 0
    fi
  fi
  # Not loaded yet on a first install: nothing to boot out.
  run launchctl bootout "gui/${uid}/${DAEMON_LABEL}" 2>/dev/null || true
  [ "$dry_run" = 1 ] || launchd_wait_gone "$DAEMON_LABEL"
  run launchctl enable "gui/${uid}/${DAEMON_LABEL}" || true
  run launchctl bootstrap "gui/${uid}" "$daemon_plist" \
    || warn "loading ${DAEMON_LABEL} failed (see ${logs_dir}/daemon.log)"
}

services_install() {
  [ -n "${HOME:-}" ] || { warn "HOME is not set: worker services skipped"; manual_start; return 0; }
  case "$os" in
    Linux) systemd_install ;;
    Darwin) launchd_install ;;
    *) warn "no worker services for ${os}"; manual_start ;;
  esac
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
want_worker=0
case "$plan" in *worker*) want_worker=1 ;; esac
if [ "$dry_run" = 1 ]; then
  if [ "$want_worker" = 1 ]; then
    # Rule 53: print the relay it would write; ask nothing.
    if [ -e "$(config_file)" ]; then echo "relay:      kept ($(config_file) exists)"
    else echo "relay:      ${relay:-none} (would run: lattice config set relay.addr ${relay:-none})"; fi
    [ "$no_service" = 1 ] || services_install
  fi
  exit 0
fi

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
if [ "$want_worker" = 1 ]; then
  configure_relay
  # Rule 56: --no-service prints how to start both daemons by hand.
  if [ "$no_service" = 1 ]; then manual_start; else services_install; fi
  echo "see docs/specs/01-pairing.md to pair a deck"
fi
