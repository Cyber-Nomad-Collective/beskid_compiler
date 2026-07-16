#!/usr/bin/env bash
# Hermetic regression test for the Linux runtime-kit evidence wrapper.
set -euo pipefail

compiler_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
trap 'rm -rf "${fixture_root}"' EXIT

prefix="${fixture_root}/prefix"
kit_root="${prefix}/lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/debug"
mkdir -p "${kit_root}/static" "${kit_root}/shared" "${fixture_root}/bin"
printf '{}\n' > "${kit_root}/abi.json"
touch "${kit_root}/static/libbeskid_runtime.a" "${kit_root}/shared/libbeskid_runtime.so"

cat > "${fixture_root}/bin/nm" <<'EOF'
#!/usr/bin/env bash
if [[ " $* " == *" --defined-only "* ]]; then
  printf '%s\n' beskid_rt_v5_process_init
else
  printf '%s\n' mmap
fi
EOF
cat > "${fixture_root}/bin/cargo" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${BESKID_LINUX_EVIDENCE_LOG}"
if [[ "${BESKID_LINUX_EVIDENCE_FAIL_ENGINE:-}" == 1 && " $* " == *" -p beskid_engine "* ]]; then
  exit 1
fi
EOF
cat > "${fixture_root}/bin/uname" <<'EOF'
#!/usr/bin/env bash
case "${1:-}" in
  -s) printf '%s\n' Linux ;;
  -m) printf '%s\n' x86_64 ;;
  *) exit 1 ;;
esac
EOF
chmod +x "${fixture_root}/bin/nm" "${fixture_root}/bin/cargo" "${fixture_root}/bin/uname"

cat > "${fixture_root}/bin/file" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'ELF 64-bit shared object'
EOF
cat > "${fixture_root}/bin/readelf" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "readelf $*"
EOF
cat > "${fixture_root}/bin/ldd" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'linux-vdso.so.1'
EOF
chmod +x "${fixture_root}/bin/file" "${fixture_root}/bin/readelf" "${fixture_root}/bin/ldd"

PATH="${fixture_root}/bin:${PATH}" \
BESKID_RUNTIME_PREFIX="${prefix}" \
BESKID_RUNTIME_KIT_PROFILE=debug \
BESKID_LINUX_EVIDENCE_LOG="${fixture_root}/calls.log" \
  "${compiler_root}/scripts/verify-native-runtime-kit-linux.sh" >/dev/null

test "$(grep -F -- '--bin beskid_runtime_provenance -- --verify' "${fixture_root}/calls.log" | wc -l | tr -d '[:space:]')" = 2
grep -F -- '-p beskid_engine --test native_runtime_kit_smoke staged_linux_runtime_kit_executes_a_canonical_entrypoint -- --ignored --exact' "${fixture_root}/calls.log" >/dev/null
grep -F -- '-p beskid_repl staged_linux_runtime_kit_evaluates_a_snippet -- --ignored --exact' "${fixture_root}/calls.log" >/dev/null

if PATH="${fixture_root}/bin:${PATH}" \
  BESKID_RUNTIME_PREFIX="${prefix}" \
  BESKID_RUNTIME_KIT_PROFILE=debug \
  BESKID_LINUX_EVIDENCE_LOG="${fixture_root}/calls.log" \
  BESKID_LINUX_EVIDENCE_FAIL_ENGINE=1 \
  "${compiler_root}/scripts/verify-native-runtime-kit-linux.sh" >"${fixture_root}/failure.log" 2>&1; then
  echo "expected Linux evidence failure" >&2
  exit 1
fi
grep -F 'Linux native runtime-kit failure diagnostics' "${fixture_root}/failure.log" >/dev/null
grep -F "file ${kit_root}/shared/libbeskid_runtime.so" "${fixture_root}/failure.log" >/dev/null
grep -F 'readelf -d' "${fixture_root}/failure.log" >/dev/null
grep -F 'readelf -Ws' "${fixture_root}/failure.log" >/dev/null
grep -F 'ldd' "${fixture_root}/failure.log" >/dev/null

echo "Linux native runtime-kit evidence wrapper test passed"
