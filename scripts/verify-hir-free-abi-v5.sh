#!/usr/bin/env bash
# Release gate for the HIR-free, ABI-v5-only production path.
#
# This gate intentionally has no allowlist.  A retired pattern in production,
# test support, or generated fixtures is still a release blocker; categories
# exist solely to make the deletion work measurable.  The scan-root override
# is test-only and never changes the workspace used for provenance verification.
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scan_root="${BESKID_HIR_FREE_SCAN_ROOT:-$workspace}"
scan_crates="$scan_root/crates"

if [[ ! -d "$scan_crates" ]]; then
  echo "HIR-free ABI-v5 gate: scan root has no crates directory: $scan_root" >&2
  exit 64
fi

failed=0
active_count=0
test_count=0
fixture_count=0
provenance_count=0

category_for_path() {
  case "$1" in
    */fixtures/*|*/fixture/*|*/generated/*)
      echo "generated/fixtures"
      ;;
    */tests/*|*/benches/*|*_test.rs|*/test_*.rs)
      echo "test support"
      ;;
    *)
      echo "active production"
      ;;
  esac
}

increment_category() {
  case "$1" in
    "active production") active_count=$((active_count + 1)) ;;
    "test support") test_count=$((test_count + 1)) ;;
    "generated/fixtures") fixture_count=$((fixture_count + 1)) ;;
  esac
}

report_matches() {
  local label="$1"
  local pattern="$2"
  local output
  local match
  local path
  local category
  local search_paths=()
  shift 2

  for path in "$@"; do
    [[ -e "$path" ]] && search_paths+=("$path")
  done
  if ((${#search_paths[@]} == 0)); then
    return 0
  fi

  output="$(rg -n --glob '*.rs' -- "$pattern" "${search_paths[@]}" || true)"
  if [[ -z "$output" ]]; then
    return 0
  fi

  echo "$label"
  while IFS= read -r match; do
    path="${match%%:*}"
    category="$(category_for_path "$path")"
    increment_category "$category"
    echo "[$category] $match"
  done <<< "$output"
  failed=1
}

report_matches \
  "HIR references remain" \
  'beskid_analysis::hir|crate::hir|\bHir[A-Z]|\bUnitHir\b|\bunit_hir(_tracked|_with_source)?\b' \
  "$scan_crates"

report_matches \
  "Rust lowering fallbacks remain" \
  '\bLowerable\b|\blower_program_with_assembly\b|\blower_node\b' \
  "$scan_crates"

report_matches \
  "legacy Rust runtime linkage remains in production consumers" \
  'beskid_runtime::|beskid_host::|beskid_runtime_bridge|register_kernel_exports|bootstrap_dispatch_handlers' \
  "$scan_crates/beskid_aot/src" \
  "$scan_crates/beskid_codegen/src" \
  "$scan_crates/beskid_engine/src" \
  "$scan_crates/beskid_cli/src" \
  "$scan_crates/beskid_repl/src"

report_matches \
  "legacy ABI dispatch or fallback discovery remains" \
  'DISPATCH_|dispatch_(tag|route|envelope)|UsePrebuilt|RuntimeLinkProfile::Minimal|BESKID_RUNTIME_ARCHIVE' \
  "$scan_crates"

verify_provenance_fixture() {
  local target="$1"
  echo "verifying ABI-v5 runtime provenance fixture: $target"
  if ! (
    cd "$workspace"
    cargo run --quiet -p beskid_abi --bin beskid_runtime_provenance -- --fixture "$target" \
      | scripts/verify-runtime-provenance.sh -
  ); then
    echo "ABI-v5 runtime provenance fixture failed: $target" >&2
    provenance_count=$((provenance_count + 1))
    failed=1
  fi
}

# These are the supported triples in the canonical ABI-v5 manifest.  Keeping
# the list explicit makes a target addition require an intentional gate update.
verify_provenance_fixture "aarch64-apple-darwin"
verify_provenance_fixture "x86_64-unknown-linux-gnu"
verify_provenance_fixture "x86_64-pc-windows-msvc"

source_total=$((active_count + test_count + fixture_count))
total_count=$((source_total + provenance_count))
echo "HIR-free ABI-v5 blocker summary: active production=$active_count; test support=$test_count; generated/fixtures=$fixture_count; source total=$source_total; provenance fixtures=$provenance_count; total=$total_count"

if (( failed != 0 )); then
  exit 1
fi

echo "HIR-free ABI-v5 retired-pattern gate passed"
