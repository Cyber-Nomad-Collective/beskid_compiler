#!/usr/bin/env bash
# Local parity with Dagger compiler-rust-gate clippy step.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo clippy --workspace --all-targets -- -D warnings
