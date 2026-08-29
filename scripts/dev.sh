#!/usr/bin/env bash
set -euo pipefail

if ! cargo watch --version >/dev/null 2>&1; then
  echo "cargo-watch is not installed; installing it now..."
  cargo install --locked cargo-watch
fi

# cargo-watch restarts running commands by default when a change is detected.
exec cargo watch \
  --watch src \
  --watch data \
  --watch Cargo.toml \
  --watch build.rs \
  --exec run
