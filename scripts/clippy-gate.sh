#!/usr/bin/env bash
# Local parity with the compiler-rust-gate clippy step
# (superrepo scripts/ci/compiler-rust-gate.sh, run via Blacksmith Testbox).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo clippy --workspace --all-targets --no-deps -- -D warnings
