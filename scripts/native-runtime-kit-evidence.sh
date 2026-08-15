#!/usr/bin/env bash
# Stable shell entry point for the native ABI-v5 runtime-kit evidence writer.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "${root}/scripts/native-runtime-kit-evidence.py" "$@"
