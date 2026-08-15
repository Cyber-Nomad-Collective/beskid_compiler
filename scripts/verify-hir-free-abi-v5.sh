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
static_only=0

case "${1:-}" in
  "") ;;
  --static-only) static_only=1 ;;
  *) echo "usage: $0 [--static-only]" >&2; exit 64 ;;
esac

if [[ ! -d "$scan_crates" ]]; then
  echo "HIR-free ABI-v5 gate: scan root has no crates directory: $scan_root" >&2
  exit 64
fi

source_scan_paths=("$scan_crates")
dependency_scan_paths=("$scan_root/Cargo.toml" "$scan_crates")
if (( static_only != 0 )) && [[ -z "${BESKID_HIR_FREE_SCAN_ROOT:-}" ]]; then
  source_scan_paths=()
  for path in \
    "$scan_crates/beskid_cli" \
    "$scan_crates/beskid_engine" \
    "$scan_crates/beskid_aot" \
    "$scan_crates/beskid_lsp" \
    "$scan_crates/beskid_repl" \
    "$scan_crates/beskid_tools" \
    "$scan_crates/beskid_tests" \
    "$scan_crates/beskid_e2e_tests" \
    "$scan_crates/beskid_queries"
  do
    [[ -e "$path" ]] && source_scan_paths+=("$path")
  done
  dependency_scan_paths=("$scan_root/Cargo.toml")
  for path in "${source_scan_paths[@]}"; do
    [[ -f "$path/Cargo.toml" ]] && dependency_scan_paths+=("$path/Cargo.toml")
  done
fi

failed=0
active_count=0
test_count=0
fixture_count=0
dependency_count=0
provenance_count=0
canonical_dispatch_count=0
deprecated_fallback_count=0

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

report_dependency_matches() {
  local output
  local match
  local search_paths=()
  local path

  for path in "$@"; do
    [[ -e "$path" ]] && search_paths+=("$path")
  done
  if ((${#search_paths[@]} == 0)); then
    return 0
  fi

  output="$(rg -n --glob 'Cargo.toml' -- 'beskid_(?:runtim[e](?:_bridge)?|hos[t])' "${search_paths[@]}" || true)"
  if [[ -z "$output" ]]; then
    return 0
  fi

  echo "retired Rust runtime dependency paths remain"
  while IFS= read -r match; do
    dependency_count=$((dependency_count + 1))
    echo "[retired dependency] $match"
  done <<< "$output"
  failed=1
}

# The ABI-v5 manifest intentionally uses dispatch vocabulary: its generated
# route table is the canonical direct-call contract consumed by ISLE.  Record
# that surface as evidence, but never mistake it for a Rust-runtime fallback.
report_canonical_dispatch_evidence() {
  local output
  local match

  output="$(rg -n --glob '*.rs' -- '\bdispatch_route_for_symbo[l]\b|\bDispatchRout[e]\b|\bDISPATCH_[A-Z0-9_]+' "$scan_crates" || true)"
  if [[ -z "$output" ]]; then
    return 0
  fi

  echo "canonical ABI-v5 dispatch evidence"
  while IFS= read -r match; do
    canonical_dispatch_count=$((canonical_dispatch_count + 1))
    echo "[canonical ABI-v5 dispatch] $match"
  done <<< "$output"
}

# Deprecated fallbacks are identified by the retired runtime boundary they
# reach, rather than by generic `DISPATCH_`/`dispatch_*` spelling.  This keeps
# canonical ABI-v5 route generation visible while rejecting the only symbols
# that can reintroduce Rust runtime dispatch or profile/archive fallback.
report_deprecated_fallback_matches() {
  local output
  local match
  local path
  local category

  output="$(rg -n --glob '*.rs' -- '\bUsePrebuilt\b|\bRuntimeLinkProfile::Minimal\b|\bBESKID_RUNTIME_ARCHIVE\b|\bbootstrap_dispatch_handlers\b|\bregister_kernel_exports\b|\binterop_dispatc[h]_(unit|usize|i64|ptr)\b' "$scan_crates" || true)"
  if [[ -z "$output" ]]; then
    return 0
  fi

  echo "deprecated ABI dispatch/fallback reachability remains"
  while IFS= read -r match; do
    path="${match%%:*}"
    category="$(category_for_path "$path")"
    increment_category "$category"
    deprecated_fallback_count=$((deprecated_fallback_count + 1))
    echo "[$category] [deprecated fallback] $match"
  done <<< "$output"
  failed=1
}

report_matches \
  "HIR references remain" \
  'beskid_analysis::hir|crate::hir|\bHir[A-Z]|\bUnitHir\b|\bunit_hir(_tracked|_with_source)?\b|\bbuild_hir_units\b|\bhir_units\b|\blower_normalize_resolve_type_spanned(_with_assembly)?\b' \
  "${source_scan_paths[@]}"

report_matches \
  "Rust lowering fallbacks remain" \
  '\bLowerable\b|\blower_program_with_assembly\b' \
  "${source_scan_paths[@]}"

report_matches \
  "legacy Rust runtime linkage remains in production consumers" \
  'beskid_runtim[e]::|beskid_hos[t]::|beskid_runtime_bridg[e]|register_kernel_exports|bootstrap_dispatch_handlers' \
  "$scan_crates/beskid_aot/src" \
  "$scan_crates/beskid_codegen/src" \
  "$scan_crates/beskid_engine/src" \
  "$scan_crates/beskid_cli/src" \
  "$scan_crates/beskid_repl/src"

report_canonical_dispatch_evidence
report_deprecated_fallback_matches

# Source scans cannot see a retired crate that is still pulled into a release
# closure through Cargo metadata.  Inspect declarations directly so the gate
# fails before a workspace build can reintroduce the Rust runtime path.
report_dependency_matches "${dependency_scan_paths[@]}"

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
if (( static_only == 0 )); then
  verify_provenance_fixture "aarch64-apple-darwin"
  verify_provenance_fixture "x86_64-unknown-linux-gnu"
  verify_provenance_fixture "x86_64-pc-windows-msvc"
else
  echo "static-only mode: skipped Cargo-backed ABI-v5 provenance fixtures"
fi

source_total=$((active_count + test_count + fixture_count))
total_count=$((source_total + dependency_count + provenance_count))
echo "HIR-free ABI-v5 blocker summary: active production=$active_count; test support=$test_count; generated/fixtures=$fixture_count; source total=$source_total; retired dependencies=$dependency_count; deprecated fallback=$deprecated_fallback_count; canonical ABI-v5 dispatch evidence=$canonical_dispatch_count; provenance fixtures=$provenance_count; total=$total_count"

if (( failed != 0 )); then
  exit 1
fi

echo "HIR-free ABI-v5 retired-pattern gate passed"
