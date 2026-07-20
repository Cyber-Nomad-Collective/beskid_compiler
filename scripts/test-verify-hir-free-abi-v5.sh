#!/usr/bin/env bash
# Regression test for the HIR retirement gate's category reporting.  The
# fixture contains one violation from each class; none is allowlisted.
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
output="$fixture_root/gate.out"
canonical_root="$(mktemp -d)"
canonical_output="$canonical_root/gate.out"
deprecated_root="$(mktemp -d)"
deprecated_output="$deprecated_root/gate.out"
trap 'rm -rf "$fixture_root" "$canonical_root" "$deprecated_root"' EXIT

mkdir -p \
  "$fixture_root/crates/example/src" \
  "$fixture_root/crates/example/tests" \
  "$fixture_root/crates/example/fixtures" \
  "$fixture_root/crates/beskid_aot/src"

cat > "$fixture_root/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/beskid_runtime_bridge"]
EOF

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
grep -F '[retired dependency]' "$output" >/dev/null
grep -F 'active production=2; test support=1; generated/fixtures=1; source total=4; retired dependencies=1' "$output" >/dev/null

# ABI-v5 keeps a generated dispatch manifest and its ISLE consumers. Those are
# evidence of the canonical direct-call boundary, not a deprecated fallback.
mkdir -p "$canonical_root/crates/example/src"
printf '%s\n' '[workspace]' > "$canonical_root/Cargo.toml"
printf '%s\n' \
  'use beskid_abi::{dispatch_route_for_symbol, DISPATCH_ENTRY_COUNT};' \
  'fn canonical_route() { let _ = (dispatch_route_for_symbol("fiber_spawn_with_cancel_slot"), DISPATCH_ENTRY_COUNT); }' \
  > "$canonical_root/crates/example/src/canonical_dispatch.rs"

if ! BESKID_HIR_FREE_SCAN_ROOT="$canonical_root" "$workspace/scripts/verify-hir-free-abi-v5.sh" >"$canonical_output" 2>&1; then
  cat "$canonical_output" >&2
  echo "expected canonical ABI-v5 dispatch evidence to pass the retirement gate" >&2
  exit 1
fi

grep -F 'canonical ABI-v5 dispatch evidence=2' "$canonical_output" >/dev/null
if grep -F 'deprecated ABI dispatch/fallback reachability remains' "$canonical_output" >/dev/null; then
  cat "$canonical_output" >&2
  echo "canonical ABI-v5 dispatch was misclassified as a deprecated fallback" >&2
  exit 1
fi

# A deprecated Rust-runtime dispatcher is a fallback reachability edge even
# when it does not use one of the canonical manifest's generic DISPATCH names.
mkdir -p "$deprecated_root/crates/example/src"
printf '%s\n' '[workspace]' > "$deprecated_root/Cargo.toml"
printf '%s\n' \
  'use beskid_runtime::bootstrap_dispatch_handlers;' \
  'fn deprecated_fallback() { bootstrap_dispatch_handlers(); }' \
  > "$deprecated_root/crates/example/src/deprecated_dispatch.rs"

if BESKID_HIR_FREE_SCAN_ROOT="$deprecated_root" "$workspace/scripts/verify-hir-free-abi-v5.sh" >"$deprecated_output" 2>&1; then
  cat "$deprecated_output" >&2
  echo "expected retirement gate to reject deprecated dispatch fallback reachability" >&2
  exit 1
fi

grep -F 'deprecated ABI dispatch/fallback reachability remains' "$deprecated_output" >/dev/null
grep -F '[deprecated fallback]' "$deprecated_output" >/dev/null

# The retirement gate must keep rejecting this symbol in synthetic input, but
# the canonical workspace must not export or reach the obsolete bootstrap at
# all. The exact-kit ABI manifest is the only dispatch authority.
if rg -n --glob '*.rs' -- '\bbootstrap_dispatch_handlers\b' "$workspace/crates"; then
  echo "retired dispatch bootstrap remains reachable in the canonical workspace" >&2
  exit 1
fi

echo "HIR-free ABI-v5 retirement gate category test passed"
