#!/usr/bin/env bash
# Build one fresh, canonical ABI-v5 runtime kit for this host and export its prefix for callers.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
profile="${BESKID_RUNTIME_KIT_PROFILE:-debug}"
case "${profile}" in
  debug | release) ;;
  *)
    echo "Unsupported BESKID_RUNTIME_KIT_PROFILE: ${profile}" >&2
    exit 1
    ;;
esac

target_root="${CARGO_TARGET_DIR:-${ROOT}/target}"
prefix="${BESKID_RUNTIME_PREFIX:-${target_root}/native-runtime-kit}"
runtime_root="${prefix}/lib/beskid-runtime/abi-5"

# A kit is immutable once published. Remove only the generated runtime subtree so each CI gate
# proves production from canonical source rather than consuming a prior bridge or stale kit.
rm -rf "${runtime_root}"
mkdir -p "${prefix}"
export BESKID_RUNTIME_PREFIX="${prefix}"

if [[ -n "${BESKID_CLI_BIN:-}" ]]; then
  cli=("${BESKID_CLI_BIN}")
else
  cli=(cargo run -q -p beskid_cli --)
fi

echo "==> Building native ABI-v5 runtime kit (${profile}) at ${prefix}"
"${cli[@]}" runtime-kit build-native-host --prefix "${prefix}" --profile "${profile}"

if [[ "$(uname -s)" == "Linux" && "$(uname -m)" == "x86_64" ]]; then
  "${ROOT}/scripts/verify-native-runtime-kit-linux.sh"
fi
