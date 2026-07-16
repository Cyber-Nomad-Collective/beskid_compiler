#!/usr/bin/env bash
# Reject CI wiring that prepares the retired Rust runtime bridge instead of a
# canonical ABI-v5 runtime kit. The gate is intentionally separate from the
# workspace test command: until every listed caller can build and install a
# native kit, invoking it reports the exact migration dependency rather than
# silently substituting an archive.
set -euo pipefail

compiler_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ci_root="${BESKID_NATIVE_RUNTIME_CI_ROOT:-$(cd "${compiler_root}/.." && pwd)}"

callers=(
  "${ci_root}/scripts/ci/compiler-rust-gate.sh"
  "${ci_root}/scripts/ci/corelib-gate.sh"
  "${ci_root}/scripts/ci/corelib-publish.sh"
  "${ci_root}/.github/workflows/compiler-gate-testbox.yml"
)

failed=0
for caller in "${callers[@]}"; do
  [[ -f "${caller}" ]] || continue
  if rg -n --with-filename --fixed-strings 'ensure-runtime-bridge.sh' "${caller}"; then
    failed=1
  fi
done

if (( failed != 0 )); then
  cat >&2 <<EOF
Native ABI-v5 runtime-kit CI migration is incomplete.

The listed caller still builds or prepares beskid_runtime_bridge. Replace each
call only after that job can build the canonical Beskid runtime through the
syntax/Salsa -> ISLE -> verified-CLIF path and install a manifest-validated
static and shared ABI-v5 kit at:
  lib/beskid-runtime/abi-5/<target>/<debug|release>/

The replacement must set BESKID_RUNTIME_PREFIX to that empty installed prefix
before invoking CLI, AOT, JIT, or corelib tests. It must not use a source-tree
archive, Rust bridge, host fallback, or profile fallback. See
docs/abi-v5-native-runtime-kit-ci.md for the per-caller acceptance criteria.
EOF
  exit 1
fi

echo "native ABI-v5 runtime-kit CI migration gate passed"
