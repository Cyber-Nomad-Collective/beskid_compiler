#!/usr/bin/env bash
# Regression test for the HIR retirement gate's category reporting.  The
# fixture contains one violation from each class; none is allowlisted.
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
output="$fixture_root/gate.out"
trap 'rm -rf "$fixture_root"' EXIT

mkdir -p \
  "$fixture_root/crates/example/src" \
  "$fixture_root/crates/example/tests" \
  "$fixture_root/crates/example/fixtures" \
  "$fixture_root/crates/beskid_aot/src"

printf '%s\n' 'use beskid_analysis::hir::Node;' > "$fixture_root/crates/example/src/production.rs"
printf '%s\n' 'trait Lowerable {}' > "$fixture_root/crates/example/tests/lowering.rs"
printf '%s\n' 'struct HirFixture;' > "$fixture_root/crates/example/fixtures/legacy.rs"
printf '%s\n' 'beskid_runtime::bootstrap();' > "$fixture_root/crates/beskid_aot/src/runtime.rs"

if BESKID_HIR_FREE_SCAN_ROOT="$fixture_root" "$workspace/scripts/verify-hir-free-abi-v5.sh" >"$output" 2>&1; then
  echo "expected retirement gate to reject its violation fixture" >&2
  exit 1
fi

grep -F '[active production]' "$output" >/dev/null
grep -F '[test support]' "$output" >/dev/null
grep -F '[generated/fixtures]' "$output" >/dev/null
grep -F 'active production=2; test support=1; generated/fixtures=1; source total=4' "$output" >/dev/null

echo "HIR-free ABI-v5 retirement gate category test passed"
