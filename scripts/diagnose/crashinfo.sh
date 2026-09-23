#!/usr/bin/env bash
# crashinfo.sh -- superseded by crashdiag.sh (same directory). Kept as a thin compatibility
# wrapper for the original `crashinfo.sh <target> [slice] [--no-rerun]` call shape. All builder
# host/container/repo configuration lives in crashdiag.sh -- see its header for the env vars.
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET="${1:?usage: crashinfo.sh <target> [slice] [--no-rerun]}"
SLICE="cov"
EXTRA=()
for arg in "${@:2}"; do
  case "$arg" in
    --no-rerun) EXTRA+=(--no-rerun) ;;
    *) SLICE="$arg" ;;
  esac
done
exec bash "$DIR/crashdiag.sh" --target "$TARGET" --slice "$SLICE" "${EXTRA[@]}"
