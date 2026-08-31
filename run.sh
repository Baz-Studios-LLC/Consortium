#!/usr/bin/env bash
# Build and run Consortium.
#
#   ./run.sh          build and launch in dev mode (hot reload)
#   ./run.sh build    produce a bundle in src-tauri/target/release/bundle
#
# The CLI is staged first on purpose. tauri.conf.json runs `bundle:cli` from
# beforeBuildCommand but has no beforeDevCommand, so a dev build otherwise
# carries no bundled CLI and the Install CLI button fails with "bundled CLI
# missing" — a confusing way to discover that dev and release differ.

set -euo pipefail
cd "$(dirname "$0")"

command -v cargo >/dev/null || {
  echo "Rust is not installed, or cargo is not on PATH. See https://rustup.rs" >&2
  exit 1
}
command -v npm >/dev/null || {
  echo "Node is not installed, or npm is not on PATH." >&2
  exit 1
}

if [ ! -d node_modules ]; then
  echo "Installing dependencies..."
  npm install
fi

echo "Staging the consortium CLI..."
npm run bundle:cli

if [ "${1:-}" = "build" ]; then
  echo "Building a release bundle..."
  npm run build
else
  echo "Launching. First run compiles Rust from scratch and takes a few minutes."
  npm run dev
fi
