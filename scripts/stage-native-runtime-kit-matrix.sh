#!/usr/bin/env bash
# Build the two native ABI-v5 profiles, atomically publish them, and retain decisive evidence.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${root}"
target_root="${CARGO_TARGET_DIR:-${root}/target}"
prefix="${BESKID_RUNTIME_PREFIX:-${target_root}/native-runtime-kit-matrix}"
runtime_root="${prefix}/lib/beskid-runtime/abi-5"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target="x86_64-unknown-linux-gnu"; static_name="libbeskid_runtime.a"; shared_name="libbeskid_runtime.so" ;;
  Darwin-arm64) target="aarch64-apple-darwin"; static_name="libbeskid_runtime.a"; shared_name="libbeskid_runtime.dylib" ;;
  MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) target="x86_64-pc-windows-msvc"; static_name="beskid_runtime.lib"; shared_name="beskid_runtime.dll" ;;
  *) echo "Unsupported native ABI-v5 runtime-kit matrix host: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

if [[ -e "${runtime_root}/${target}" ]]; then
  echo "BESKID_RUNTIME_PREFIX must be an empty runtime-kit destination: ${prefix}" >&2
  exit 1
fi
mkdir -p "${prefix}"

if [[ -n "${BESKID_CLI_BIN:-}" ]]; then cli=("${BESKID_CLI_BIN}"); else cli=(cargo run -q -p beskid_cli --); fi

work="$(mktemp -d "${TMPDIR:-/tmp}/beskid-native-runtime-kit-matrix.XXXXXX")"
evidence_dir="${BESKID_RUNTIME_KIT_EVIDENCE_DIR:-${prefix}/evidence}"
export BESKID_RUNTIME_KIT_EVIDENCE_DIR="${evidence_dir}"
evidence_helper="${root}/scripts/native-runtime-kit-evidence.sh"

finish_evidence() {
  local exit_code=$?
  trap - EXIT
  if [[ ${exit_code} -eq 0 ]]; then
    "${evidence_helper}" finish passed 0 "${target}" "${runtime_root}/${target}" || exit_code=$?
  else
    "${evidence_helper}" finish failed "${exit_code}" "${target}" "${runtime_root}/${target}" || true
  fi
  rm -rf "${work}"
  exit "${exit_code}"
}
trap finish_evidence EXIT

stage_profile() {
  local profile="$1"
  "${cli[@]}" runtime-kit build-native-host --prefix "${work}/${profile}-source" --profile "${profile}"
}

symbol_tool="${CR_NM:-${LLVM_NM:-}}"
[[ -n "${symbol_tool}" ]] || symbol_tool="$(command -v cr_nm || true)"
[[ -n "${symbol_tool}" ]] || symbol_tool="$(command -v llvm-nm || true)"
if [[ -z "${symbol_tool}" ]]; then
  echo "Native runtime-kit provenance requires cr_nm or llvm-nm; set CR_NM or LLVM_NM" >&2
  exit 1
fi
"${evidence_helper}" init "${target}" "${symbol_tool}" "${root}"

pe_symbol_tool="${LLVM_READOBJ:-}"
if [[ "${target}" == "x86_64-pc-windows-msvc" && -z "${pe_symbol_tool}" ]]; then
  pe_symbol_tool="$(command -v llvm-readobj || command -v llvm-readobj.exe || true)"
fi
if [[ "${target}" == "x86_64-pc-windows-msvc" && -z "${pe_symbol_tool}" ]]; then
  echo "Windows shared runtime provenance requires llvm-readobj; set LLVM_READOBJ to the native tool path" >&2
  exit 1
fi

write_provenance() {
  local profile="$1" linkage="$2" library="$3"
  local profile_root="${work}/${profile}-source/lib/beskid-runtime/abi-5/${target}/${profile}"
  local symbols="${work}/${profile}-${linkage}.symbols"
  local raw_defined="${evidence_dir}/symbols/raw/${profile}-${linkage}-defined.txt"
  local raw_undefined="${evidence_dir}/symbols/raw/${profile}-${linkage}-undefined.txt"
  local normalized="${evidence_dir}/symbols/normalized/${profile}-${linkage}.symbols"
  mkdir -p "${evidence_dir}/symbols/raw" "${evidence_dir}/symbols/normalized"
  [[ -f "${profile_root}/static/${static_name}" ]] || { echo "Missing native static runtime for ${profile}" >&2; exit 1; }
  [[ -f "${profile_root}/shared/${shared_name}" ]] || { echo "Missing native shared runtime for ${profile}" >&2; exit 1; }
  mkdir -p "$(dirname "${raw_defined}")" "$(dirname "${raw_undefined}")" "$(dirname "${normalized}")"
  "${symbol_tool}" --extern-only --defined-only --format=posix "${library}" >"${raw_defined}"
  "${symbol_tool}" --extern-only --undefined-only --format=posix "${library}" >"${raw_undefined}"
  {
    printf 'target=%s\n' "${target}"
    if [[ "${target}" == "x86_64-pc-windows-msvc" && "${linkage}" == "shared" ]]; then
      "${pe_symbol_tool}" --coff-exports "${library}" \
        | awk '/^[[:space:]]*Name: / { print "defined=" $2 }'
      "${pe_symbol_tool}" --coff-imports "${library}" \
        | awk '/^[[:space:]]*Symbol: / { print "undefined=" $2 }'
    else
      awk 'NF >= 2 { print "defined=" $1 }' "${raw_defined}"
      awk 'NF >= 2 { print "undefined=" $1 }' "${raw_undefined}"
    fi
  } >"${symbols}"
  if [[ "${target}" == "aarch64-apple-darwin" ]]; then
    sed -i.bak -E 's/^(defined|undefined)=_/\1=/' "${symbols}"
    rm -f "${symbols}.bak"
  fi
  [[ -s "${symbols}" ]] || { echo "Empty provenance symbol report: ${symbols}" >&2; exit 1; }
  cp "${symbols}" "${normalized}"
}

run_smoke() {
  local profile="$1" consumer="$2" linkage="$3"; shift 3
  local output="${evidence_dir}/smokes/${profile}-${consumer}.log" command_text status exit_code=0
  mkdir -p "$(dirname "${output}")"
  printf -v command_text '%q ' "$@"
  if "$@" > >(tee "${output}") 2>&1; then status=passed; else status=failed; exit_code=$?; fi
  "${evidence_helper}" smoke "${target}" "${profile}" "${consumer}" "${linkage}" "${status}" "${exit_code}" "smokes/${profile}-${consumer}.log" "${command_text% }"
  return "${exit_code}"
}

stage_profile debug
stage_profile release
debug_root="${work}/debug-source/lib/beskid-runtime/abi-5/${target}/debug"
release_root="${work}/release-source/lib/beskid-runtime/abi-5/${target}/release"
write_provenance debug static "${debug_root}/static/${static_name}"
write_provenance debug shared "${debug_root}/shared/${shared_name}"
write_provenance release static "${release_root}/static/${static_name}"
write_provenance release shared "${release_root}/shared/${shared_name}"
matrix_args=(runtime-kit build-matrix --prefix "${prefix}" --target "${target}"
  --debug-static-library "${debug_root}/static/${static_name}" --debug-shared-library "${debug_root}/shared/${shared_name}"
  --release-static-library "${release_root}/static/${static_name}" --release-shared-library "${release_root}/shared/${shared_name}"
  --debug-static-provenance-symbol-list "${work}/debug-static.symbols" --debug-shared-provenance-symbol-list "${work}/debug-shared.symbols"
  --release-static-provenance-symbol-list "${work}/release-static.symbols" --release-shared-provenance-symbol-list "${work}/release-shared.symbols")
if [[ "${target}" == "x86_64-pc-windows-msvc" ]]; then
  debug_import="${debug_root}/shared/beskid_runtime_import.lib"; release_import="${release_root}/shared/beskid_runtime_import.lib"
  [[ -f "${debug_import}" && -f "${release_import}" ]] || { echo "Windows ABI-v5 runtime kit requires debug and release import libraries" >&2; exit 1; }
  matrix_args+=(--debug-shared-import-library "${debug_import}" --release-shared-import-library "${release_import}")
fi
"${cli[@]}" "${matrix_args[@]}"

export BESKID_RUNTIME_PREFIX="${prefix}"
for profile in debug release; do
  export BESKID_RUNTIME_KIT_PROFILE="${profile}"
  run_smoke "${profile}" jit shared cargo test -p beskid_engine --test native_runtime_kit_smoke staged_runtime_kit_executes_a_canonical_entrypoint -- --ignored --exact
  run_smoke "${profile}" aot static cargo test -p beskid_aot --test abi_v5_runtime_kit staged_runtime_kit_links_and_executes_with_the_canonical_static_archive -- --ignored --exact
  run_smoke "${profile}" repl shared cargo test -p beskid_repl eval::tests::staged_native_runtime_kit_evaluates_a_snippet -- --ignored --exact
done

smoke_source="${work}/runtime-kit-cli-smoke.bd"
cat >"${smoke_source}" <<'EOF'
use Testing.Assert;

pub i64 Main() {
  i64 semanticValue = 42;
  Assert.Equal(semanticValue, 42, "native runtime-kit CLI semantic value mismatch");
  return 0;
}
EOF
export BESKID_RUNTIME_KIT_PROFILE=debug
run_smoke debug cli shared "${cli[@]}" run "${smoke_source}" --plain

echo "Native ABI-v5 runtime-kit matrix evidence passed for ${target} at ${prefix}"
