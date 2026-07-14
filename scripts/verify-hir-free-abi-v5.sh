#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace"

failed=0

report_matches() {
  local label="$1"
  local pattern="$2"
  shift 2
  local output
  output="$(rg -n --glob '*.rs' "$pattern" "$@" || true)"
  if [[ -n "$output" ]]; then
    echo "$label"
    echo "$output"
    failed=1
  fi
}

report_matches \
  "active HIR references remain" \
  'beskid_analysis::hir|crate::hir|\bHir[A-Z]|\bUnitHir\b|\bunit_hir(_tracked|_with_source)?\b' \
  crates

report_matches \
  "Rust lowering fallbacks remain" \
  '\bLowerable\b|\blower_program_with_assembly\b|\blower_node\b' \
  crates

report_matches \
  "legacy Rust runtime linkage remains in production consumers" \
  'beskid_runtime::|beskid_host::|beskid_runtime_bridge|register_kernel_exports|bootstrap_dispatch_handlers' \
  crates/beskid_aot/src \
  crates/beskid_codegen/src \
  crates/beskid_engine/src \
  crates/beskid_cli/src \
  crates/beskid_repl/src

report_matches \
  "legacy ABI dispatch or fallback discovery remains" \
  'DISPATCH_|dispatch_(tag|route|envelope)|UsePrebuilt|RuntimeLinkProfile::Minimal|BESKID_RUNTIME_ARCHIVE' \
  crates

if (( failed != 0 )); then
  exit 1
fi

echo "HIR-free ABI-v5 retired-pattern gate passed"
