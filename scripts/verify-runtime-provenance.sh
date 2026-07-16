#!/usr/bin/env bash
# Verify an explicit ABI-v5 runtime symbol-list fixture. This script intentionally does not invoke
# nm, otool, dumpbin, or any host-specific binary parser; platform adapters must emit the same
# line-oriented input consumed here before this release gate is run.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ $# -ne 1 ]]; then
  echo "usage: $0 <symbol-list>|-" >&2
  exit 64
fi

cd "$ROOT"
cargo run --quiet -p beskid_abi --bin beskid_runtime_provenance -- --verify "$1"
