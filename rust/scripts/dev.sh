#!/usr/bin/env bash
# Development server: the Rust backend on $PORT plus the Vite dev server (HMR) on
# $VITE_PORT, which proxies everything but its own assets to the backend.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
export PORT="${PORT:-4000}" THE_GATHERING_ENV="${THE_GATHERING_ENV:-dev}"
(cd rust && cargo build --locked --bin the-gathering)
aube run dev &
vite=$!
trap 'kill "$vite" 2>/dev/null || true' EXIT
rust/target/debug/the-gathering serve
