#!/usr/bin/env bash
set -euo pipefail

# rracer setup script
# - Ensures rustup is installed and on PATH
# - Adds the wasm32-unknown-unknown target
# - Installs Trunk (via cargo)
# - Builds the WASM web client into web/dist using the rustup toolchain
# - Optional: runs the server (cargo run -p server)

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEB_DIR="$ROOT_DIR/web"

usage() {
  cat <<EOF
Usage: bash setup.sh [options]

Options:
  --install-only   Only install tooling (rustup, wasm32 target, trunk); skip build
  --build-only     Only build the web client; do not attempt installs
  --run            Run the server after a successful build
  -h, --help       Show this help

Notes:
  - This script prefers the rustup-managed toolchain and forces Trunk/Cargo to use it
    to avoid Homebrew cargo/rustc mismatches that break wasm builds.
EOF
}

log() { echo "==> $*"; }
warn() { echo "[warn] $*" >&2; }
err() { echo "[error] $*" >&2; exit 1; }

need_cmd() { command -v "$1" >/dev/null 2>&1; }

# Return the bin dir that contains the rustup-managed rustc/cargo
rustup_bin_dir() {
  if need_cmd rustup; then
    local bin
    bin="$(dirname "$(rustup which rustc 2>/dev/null || true)")"
    if [[ -n "${bin}" && -x "${bin}/cargo" ]]; then
      echo "${bin}"
      return 0
    fi
  fi
  # Fallback: ~/.cargo/bin may contain shims
  if [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    echo "$HOME/.cargo/bin"
    return 0
  fi
  return 1
}

ensure_rustup() {
  if need_cmd rustup; then
    log "rustup present: $(rustup --version)"
    return
  fi
  log "Installing rustup (via Homebrew if available)..."
  if need_cmd brew; then
    brew install rustup-init
    rustup-init -y
  else
    curl https://sh.rustup.rs -sSf | sh -s -- -y
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
  need_cmd rustup || err "rustup install appears to have failed"
}

ensure_wasm_target() {
  log "Adding wasm32-unknown-unknown target (idempotent)"
  rustup target add wasm32-unknown-unknown || true
}

ensure_trunk() {
  if need_cmd trunk; then
    log "trunk present: $(trunk --version)"
    return
  fi
  log "Installing trunk via cargo..."
  local rb
  rb="$(rustup_bin_dir || true)"
  if [[ -n "${rb}" ]]; then
    PATH="${rb}:$PATH" cargo install trunk
  else
    cargo install trunk
  fi
  need_cmd trunk || err "Trunk install failed"
}

build_web() {
  [[ -d "$WEB_DIR" ]] || err "web directory not found at $WEB_DIR"
  local rb
  rb="$(rustup_bin_dir || true)"
  log "Building web (WASM) with Trunk..."
  pushd "$WEB_DIR" >/dev/null
  if [[ -n "${rb}" ]]; then
    PATH="${rb}:$PATH" trunk build --release
  else
    warn "rustup bin not found; using current PATH for trunk"
    trunk build --release
  fi
  popd >/dev/null
  [[ -f "$WEB_DIR/dist/index.html" ]] || err "Trunk build completed but dist/index.html not found"
  log "WASM client built to $WEB_DIR/dist"
}

run_server() {
  local rb
  rb="$(rustup_bin_dir || true)"
  log "Starting server (Ctrl-C to stop)..."
  if [[ -n "${rb}" ]]; then
    PATH="${rb}:$PATH" cargo run -p server
  else
    cargo run -p server
  fi
}

INSTALL_ONLY=0
BUILD_ONLY=0
RUN_SERVER=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --install-only) INSTALL_ONLY=1 ;;
    --build-only)   BUILD_ONLY=1 ;;
    --run)          RUN_SERVER=1 ;;
    -h|--help)      usage; exit 0 ;;
    *) warn "Unknown arg: $1" ;;
  esac
  shift
done

# Prefer rustup-managed toolchain in this script
if rb="$(rustup_bin_dir 2>/dev/null)"; then
  export PATH="$rb:$PATH"
fi

if [[ "$BUILD_ONLY" -ne 1 ]]; then
  ensure_rustup
  ensure_wasm_target
  ensure_trunk
  [[ "$INSTALL_ONLY" -eq 1 ]] && { log "Install complete"; exit 0; }
fi

build_web
[[ "$RUN_SERVER" -eq 1 ]] && run_server || true

log "Done. Server will serve web/dist automatically if present."
