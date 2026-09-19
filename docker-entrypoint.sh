#!/bin/sh
set -eu

DATA_DIR="${DATA_DIR:-/data}"

mkdir -p "$DATA_DIR"

# Take ownership of a fresh or externally-owned volume once; on normal
# restarts only the directory itself needs checking.
if [ "$(stat -c %U "$DATA_DIR")" != "app" ]; then
  chown -R app:app "$DATA_DIR"
fi

exec su-exec app "$@"
