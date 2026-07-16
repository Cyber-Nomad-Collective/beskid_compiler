#!/usr/bin/env bash
# Regression test for the explicit CI bridge-retirement diagnostic.
set -euo pipefail

compiler_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
output="${fixture_root}/gate.out"
trap 'rm -rf "${fixture_root}"' EXIT

mkdir -p "${fixture_root}/scripts/ci" "${fixture_root}/.github/workflows"
printf '%s\n' 'bash scripts/ensure-runtime-bridge.sh' > "${fixture_root}/scripts/ci/compiler-rust-gate.sh"

if BESKID_NATIVE_RUNTIME_CI_ROOT="${fixture_root}" \
  "${compiler_root}/scripts/verify-native-runtime-kit-ci.sh" >"${output}" 2>&1; then
  echo "expected native runtime-kit CI gate to reject bridge setup" >&2
  exit 1
fi

grep -F 'compiler-rust-gate.sh:1:bash scripts/ensure-runtime-bridge.sh' "${output}" >/dev/null
grep -F 'Native ABI-v5 runtime-kit CI migration is incomplete.' "${output}" >/dev/null
grep -F 'BESKID_RUNTIME_PREFIX' "${output}" >/dev/null

printf '%s\n' 'beskid runtime-kit build --target x86_64-unknown-linux-gnu' > \
  "${fixture_root}/scripts/ci/compiler-rust-gate.sh"
BESKID_NATIVE_RUNTIME_CI_ROOT="${fixture_root}" \
  "${compiler_root}/scripts/verify-native-runtime-kit-ci.sh" >"${output}"
grep -F 'native ABI-v5 runtime-kit CI migration gate passed' "${output}" >/dev/null

echo "native ABI-v5 runtime-kit CI migration gate test passed"
