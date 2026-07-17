#!/usr/bin/env bash
# Verify the staged Linux ABI-v5 runtime-kit boundary before compiler execution tests run.
set -euo pipefail

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  echo "Linux native runtime-kit evidence requires an x86_64 Linux host" >&2
  exit 1
fi

compiler_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prefix="${BESKID_RUNTIME_PREFIX:-${CARGO_TARGET_DIR:-${compiler_root}/target}/native-runtime-kit}"
profile="${BESKID_RUNTIME_KIT_PROFILE:-debug}"
case "${profile}" in
  debug | release) ;;
  *)
    echo "Unsupported BESKID_RUNTIME_KIT_PROFILE: ${profile}" >&2
    exit 1
    ;;
esac

target="x86_64-unknown-linux-gnu"
kit_root="${prefix}/lib/beskid-runtime/abi-5/${target}/${profile}"
static_library="${kit_root}/static/libbeskid_runtime.a"
shared_library="${kit_root}/shared/libbeskid_runtime.so"
[[ -f "${kit_root}/abi.json" ]] || { echo "Missing staged runtime metadata: ${kit_root}/abi.json" >&2; exit 1; }
[[ -f "${static_library}" ]] || { echo "Missing staged static runtime library: ${static_library}" >&2; exit 1; }
[[ -f "${shared_library}" ]] || { echo "Missing staged shared runtime library: ${shared_library}" >&2; exit 1; }
command -v nm >/dev/null || { echo "Linux runtime-kit evidence requires GNU nm" >&2; exit 1; }

dump_command() {
  local label="$1"
  shift
  printf '\n==> %s\n' "${label}" >&2
  "$@" >&2 || true
}

runtime_failure_diagnostics() {
  local status=$?
  trap - ERR
  set +e
  printf '\nLinux native runtime-kit failure diagnostics (status %s)\n' "${status}" >&2
  printf 'shared artifact: %s\n' "${shared_library}" >&2
  if command -v file >/dev/null; then
    dump_command "file ${shared_library}" file "${shared_library}"
  fi
  if command -v readelf >/dev/null; then
    dump_command "readelf -d ${shared_library}" readelf -d "${shared_library}"
    dump_command "readelf -Ws ${shared_library}" readelf -Ws "${shared_library}"
  elif command -v llvm-readelf >/dev/null; then
    dump_command "llvm-readelf -d ${shared_library}" llvm-readelf -d "${shared_library}"
    dump_command "llvm-readelf -Ws ${shared_library}" llvm-readelf -Ws "${shared_library}"
  elif command -v objdump >/dev/null; then
    dump_command "objdump -p ${shared_library}" objdump -p "${shared_library}"
    dump_command "objdump -T ${shared_library}" objdump -T "${shared_library}"
  else
    echo "no ELF dynamic-section or symbol-table tool is available" >&2
  fi
  if command -v ldd >/dev/null; then
    dump_command "ldd ${shared_library}" ldd "${shared_library}"
  else
    echo "ldd is unavailable" >&2
  fi
  exit "${status}"
}

trap runtime_failure_diagnostics ERR

evidence_root="$(mktemp -d)"
trap 'rm -rf "${evidence_root}"' EXIT

audit_artifact() {
  local label="$1"
  local artifact="$2"
  local dynamic="$3"
  local symbols="${evidence_root}/${label}.symbols"
  local verifier="--verify"
  {
    printf 'target=%s\n' "${target}"
    if [[ "${dynamic}" == "yes" ]]; then
      nm -D --defined-only -j "${artifact}" | sed '/^$/d; s/^/defined=/'
      nm -D --undefined-only -j "${artifact}" | sed '/^$/d; s/^/undefined=/'
    else
      nm -g --defined-only -j "${artifact}" | sed '/^$/d; s/^/defined=/'
      # An archive has no linked undefined-symbol boundary: `nm -u` reports references made by
      # every member object, including the dynamic-TLS helper that is only an ELF shared-object
      # loader import. Audit the archive's public definitions here; audit linked imports only on
      # the shared runtime below, where --verify-shared applies the narrow loader policy.
    fi
  } > "${symbols}"
  if [[ "${dynamic}" == "yes" ]]; then
    verifier="--verify-shared"
  fi
  cargo run -q -p beskid_abi --bin beskid_runtime_provenance -- "${verifier}" "${symbols}"
}

audit_artifact static "${static_library}" no
audit_artifact shared "${shared_library}" yes

export BESKID_RUNTIME_PREFIX="${prefix}"
export BESKID_RUNTIME_KIT_PROFILE="${profile}"
cargo test -p beskid_engine --test native_runtime_kit_smoke \
  staged_linux_runtime_kit_executes_a_canonical_entrypoint -- --ignored --exact
cargo test -p beskid_repl staged_linux_runtime_kit_evaluates_a_snippet -- --ignored --exact

echo "Linux native runtime-kit provenance and staged Engine/REPL evidence passed"
