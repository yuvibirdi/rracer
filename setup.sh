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
  --install-only     Only install Rust tooling (rustup, wasm32 target, trunk); skip build
  --build-only       Only build the web client; do not attempt installs
  --run              Run the server after a successful build
  -r, --release      Build (web+server) in release mode (default)
  -d, --debug        Build (web+server) in debug mode (enables testing UI)
  --db-setup         Install/start Postgres (Homebrew), create local DB, and write .env
  --ingest-file PATH Ingest passages from URLs listed in PATH (requires DATABASE_URL)
  -h, --help       Show this help

Notes:
  - This script prefers the rustup-managed toolchain and forces Trunk/Cargo to use it
    to avoid Homebrew cargo/rustc mismatches that break wasm builds.
  - Debug builds enable the in-app testing UI (gated by cfg!(debug_assertions)).
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
  local mode_msg
  if [[ "$BUILD_PROFILE" == "release" ]]; then mode_msg="release"; else mode_msg="debug"; fi
  log "Building web (WASM) with Trunk in $mode_msg mode..."
  pushd "$WEB_DIR" >/dev/null
  if [[ -n "${rb}" ]]; then
    if [[ "$BUILD_PROFILE" == "release" ]]; then
      PATH="${rb}:$PATH" trunk build --release
    else
      PATH="${rb}:$PATH" trunk build
    fi
  else
    warn "rustup bin not found; using current PATH for trunk"
    if [[ "$BUILD_PROFILE" == "release" ]]; then
      trunk build --release
    else
      trunk build
    fi
  fi
  popd >/dev/null
  [[ -f "$WEB_DIR/dist/index.html" ]] || err "Trunk build completed but dist/index.html not found"
  log "WASM client built to $WEB_DIR/dist"
}

run_server() {
  local rb
  rb="$(rustup_bin_dir || true)"
  local mode_msg
  if [[ "$BUILD_PROFILE" == "release" ]]; then mode_msg="release"; else mode_msg="debug"; fi
  log "Starting server in $mode_msg mode (Ctrl-C to stop)..."
  # Load .env if present for DATABASE_URL
  if [[ -f "$ROOT_DIR/.env" ]]; then
    log "Loading env from $ROOT_DIR/.env"
    # shellcheck disable=SC2046
    set -a; source "$ROOT_DIR/.env"; set +a
  fi
  if [[ -n "${rb}" ]]; then
    if [[ "$BUILD_PROFILE" == "release" ]]; then
      PATH="${rb}:$PATH" cargo run -p server --release --bin server
    else
      PATH="${rb}:$PATH" cargo run -p server --bin server
    fi
  else
    if [[ "$BUILD_PROFILE" == "release" ]]; then
      cargo run -p server --release --bin server
    else
      cargo run -p server --bin server
    fi
  fi
}

ensure_postgres() {
  if command -v psql >/dev/null 2>&1; then
    log "psql present: $(psql --version)"
  else
    if need_cmd brew; then
      log "Installing postgresql via Homebrew..."
      brew install postgresql@16 || brew install postgresql || true
    else
      warn "Homebrew not found; please install Postgres manually. Skipping install."
      return
    fi
  fi

  if need_cmd brew; then
    if brew list --versions postgresql@16 >/dev/null 2>&1; then
      log "Starting postgresql@16 via brew services"
      brew services start postgresql@16 || true
    elif brew list --versions postgresql >/dev/null 2>&1; then
      log "Starting postgresql via brew services"
      brew services start postgresql || true
    fi
  fi
}

create_db_and_env() {
  local db_name
  db_name="rracer"
  log "Ensuring database '$db_name' exists"
  if command -v psql >/dev/null 2>&1; then
    psql -d postgres -v ON_ERROR_STOP=1 -c "CREATE DATABASE ${db_name};" 2>/dev/null || \
      log "Database '${db_name}' already exists (ok)"
    local url
    # Default to peer auth with current user
    url="postgres://localhost/${db_name}"
    printf "DATABASE_URL=%s\n" "$url" > "$ROOT_DIR/.env"
    log "Wrote $ROOT_DIR/.env with DATABASE_URL (edit if needed)"
  else
    warn "psql not available; cannot create DB automatically"
  fi
}

ingest_file() {
  local file
  file="$1"
  [[ -f "$file" ]] || err "ingest file '$file' not found"
  # Load env to get DATABASE_URL
  if [[ -f "$ROOT_DIR/.env" ]]; then
    set -a; source "$ROOT_DIR/.env"; set +a
  fi
  [[ -n "${DATABASE_URL:-}" ]] || err "DATABASE_URL is required for ingestion; set it or run --db-setup first"
  local rb
  rb="$(rustup_bin_dir || true)"
  log "Ingesting passages from $file"
  if [[ -n "${rb}" ]]; then
    PATH="${rb}:$PATH" cargo run -p server --bin ingest -- --file "$file"
  else
    cargo run -p server --bin ingest -- --file "$file"
  fi
}

INSTALL_ONLY=0
BUILD_ONLY=0
RUN_SERVER=0
DB_SETUP=0
INGEST_PATH=""
# Default to release to match previous behavior
BUILD_PROFILE="release"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --install-only) INSTALL_ONLY=1 ;;
    --build-only)   BUILD_ONLY=1 ;;
    --run)          RUN_SERVER=1 ;;
  -r|--release)   BUILD_PROFILE="release" ;;
  -d|--debug)     BUILD_PROFILE="debug" ;;
    --db-setup)     DB_SETUP=1 ;;
    --ingest-file)
      shift
      [[ $# -gt 0 ]] || err "--ingest-file requires a path"
      INGEST_PATH="$1"
      ;;
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
  if [[ "$DB_SETUP" -eq 1 ]]; then
    ensure_postgres
    create_db_and_env
  fi
  [[ "$INSTALL_ONLY" -eq 1 ]] && { log "Install complete"; exit 0; }
fi

build_web
[[ -n "$INGEST_PATH" ]] && ingest_file "$INGEST_PATH"
[[ "$RUN_SERVER" -eq 1 ]] && run_server || true

log "Done. Server will serve web/dist automatically if present."
