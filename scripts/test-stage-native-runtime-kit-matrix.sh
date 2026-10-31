#!/usr/bin/env bash
# Regression test: the matrix script must run Cargo from the compiler workspace,
# even when GitHub Actions invokes it from the superproject checkout.
set -euo pipefail

compiler_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
trap 'rm -rf "${fixture_root}"' EXIT

mkdir -p "${fixture_root}/bin"
cat >"${fixture_root}/bin/uname" <<'EOF'
#!/usr/bin/env bash
case "$1" in
  -s) printf '%s\n' "${BESKID_MATRIX_UNAME_S}" ;;
  -m) printf '%s\n' "${BESKID_MATRIX_UNAME_M}" ;;
  *) exit 1 ;;
esac
EOF
chmod +x "${fixture_root}/bin/uname"

cat >"${fixture_root}/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$PWD" >>"${BESKID_MATRIX_CARGO_CWDS}"
printf '%s\n' "$*" >>"${BESKID_MATRIX_CARGO_CALLS}"
if [[ "$1" == "run" ]]; then
  all_args="$*"
  prefix=""
  profile=""
  debug_static_symbols=""
  debug_shared_symbols=""
  release_static_symbols=""
  release_shared_symbols=""
  debug_static_library="" debug_shared_library="" debug_import_library=""
  release_static_library="" release_shared_library="" release_import_library=""
  while (($#)); do
    case "$1" in
      --prefix) prefix="$2"; shift 2 ;;
      --debug-static-library) debug_static_library="$2"; shift 2 ;;
      --debug-shared-library) debug_shared_library="$2"; shift 2 ;;
      --release-static-library) release_static_library="$2"; shift 2 ;;
      --release-shared-library) release_shared_library="$2"; shift 2 ;;
      --debug-shared-import-library) debug_import_library="$2"; shift 2 ;;
      --release-shared-import-library) release_import_library="$2"; shift 2 ;;
      --profile) profile="$2"; shift 2 ;;
      --debug-static-provenance-symbol-list) debug_static_symbols="$2"; shift 2 ;;
      --debug-shared-provenance-symbol-list) debug_shared_symbols="$2"; shift 2 ;;
      --release-static-provenance-symbol-list) release_static_symbols="$2"; shift 2 ;;
      --release-shared-provenance-symbol-list) release_shared_symbols="$2"; shift 2 ;;
      *) shift ;;
    esac
  done
  if [[ "${all_args}" == *" runtime-kit build-native-host "* && -n "${prefix}" && -n "${profile}" ]]; then
    target="${BESKID_MATRIX_TARGET}"
    root="${prefix}/lib/beskid-runtime/abi-5/${target}/${profile}"
    mkdir -p "${root}/static" "${root}/shared"
    : >"${root}/static/${BESKID_MATRIX_STATIC_NAME}"
    : >"${root}/shared/${BESKID_MATRIX_SHARED_NAME}"
    if [[ "${target}" == "x86_64-pc-windows-msvc" ]]; then
      : >"${root}/shared/beskid_runtime_import.lib"
    fi
  elif [[ "${all_args}" == *" runtime-kit build-matrix "* ]]; then
    test -n "${debug_static_symbols}"
    test -n "${debug_shared_symbols}"
    test -n "${release_static_symbols}"
    test -n "${release_shared_symbols}"
    cp "${debug_static_symbols}" "${BESKID_MATRIX_CAPTURE_DIR}/debug-static.symbols"
    cp "${debug_shared_symbols}" "${BESKID_MATRIX_CAPTURE_DIR}/debug-shared.symbols"
    cp "${release_static_symbols}" "${BESKID_MATRIX_CAPTURE_DIR}/release-static.symbols"
    cp "${release_shared_symbols}" "${BESKID_MATRIX_CAPTURE_DIR}/release-shared.symbols"
    # Publish the kit as the real build-matrix does: each profile's libraries and the ABI-v5
    # metadata (abi.json) that evidence finalization verifies against them.
    for profile in debug release; do
      root="${prefix}/lib/beskid-runtime/abi-5/${BESKID_MATRIX_TARGET}/${profile}"
      mkdir -p "${root}/static" "${root}/shared"
      static_library="${profile}_static_library" shared_library="${profile}_shared_library"
      import_library="${profile}_import_library"
      cp "${!static_library}" "${root}/static/${BESKID_MATRIX_STATIC_NAME}"
      cp "${!shared_library}" "${root}/shared/${BESKID_MATRIX_SHARED_NAME}"
      if [[ -n "${!import_library}" ]]; then
        cp "${!import_library}" "${root}/shared/beskid_runtime_import.lib"
      fi
      python3 - "${root}" "${BESKID_MATRIX_TARGET}" "${profile}" <<'PY'
import hashlib, json, pathlib, sys
root, target, profile = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
def artifact(relative):
    return {"relative_path": relative, "sha256": hashlib.sha256((root / relative).read_bytes()).hexdigest()}
artifacts = {}
for key, directory in (("static_library", "static"), ("shared_library", "shared")):
    library = next(path for path in sorted((root / directory).iterdir()) if path.name != "beskid_runtime_import.lib")
    artifacts[key] = artifact(f"{directory}/{library.name}")
if (root / "shared" / "beskid_runtime_import.lib").is_file():
    artifacts["shared_import_library"] = artifact("shared/beskid_runtime_import.lib")
metadata = {"schema_version": 1, "abi_version": 5, "target": {"triple": target}, "profile": profile,
            "layout_hash": "0" * 64, "source_hash": "1" * 64, "artifacts": artifacts}
(root / "abi.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
PY
    done
  elif [[ "${all_args}" == *" -- run "* ]]; then
    exit 42
  fi
elif [[ "$1" == "test" ]]; then
  printf 'running %s test\n' "${BESKID_MATRIX_TESTS_RUN:-1}"
  printf 'test result: ok. %s passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n' "${BESKID_MATRIX_TESTS_RUN:-1}"
fi
EOF
chmod +x "${fixture_root}/bin/cargo"

cat >"${fixture_root}/bin/llvm-nm" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"${BESKID_MATRIX_NM_CALLS}"
if [[ " $* " == *".dll "* ]]; then
  # The raw report of a DLL may come from llvm-nm; its provenance list must not.
  printf 'nm_dll_symbol T 0\n'
  exit 0
fi
if [[ " $* " == *" --undefined-only "* ]]; then
  case "${BESKID_MATRIX_TARGET}" in
    x86_64-pc-windows-msvc) printf 'TlsAlloc U 0\n' ;;
    # Only Mach-O spells C symbols with a leading underscore.
    aarch64-apple-darwin) printf '_clock_gettime U 0\n' ;;
    *) printf 'clock_gettime U 0\n' ;;
  esac
else
  case "${BESKID_MATRIX_TARGET}" in
    aarch64-apple-darwin) printf '_beskid_rt_v5_entrypoint T 0\n' ;;
    *) printf 'beskid_rt_v5_entrypoint T 0\n' ;;
  esac
fi
EOF
chmod +x "${fixture_root}/bin/llvm-nm"

cat >"${fixture_root}/bin/llvm-readobj" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"${BESKID_MATRIX_READOBJ_CALLS}"
if [[ " $* " == *" --coff-exports "* ]]; then
  cat <<'OUTPUT'
Export {
  Ordinal: 1
  Name: beskid_rt_v5_entrypoint
  RVA: 0x1000
}
OUTPUT
elif [[ " $* " == *" --coff-imports "* ]]; then
  cat <<'OUTPUT'
Import {
  Name: KERNEL32.dll
  Symbol: TlsAlloc (9)
}
OUTPUT
fi
EOF
chmod +x "${fixture_root}/bin/llvm-readobj"

prefix="${fixture_root}/prefix"
mkdir -p "${fixture_root}/captured-symbols"

assert_lifecycle_smokes() {
  while IFS=':' read -r consumer test_name; do
    test "$(grep -Fc -- "test --locked -p beskid_engine --test engine foundation_io_native::${test_name} -- --exact --include-ignored" "${fixture_root}/cargo-calls")" = 2
  done <<'EOF'
native-executable-streams:staged_runtime_kit_emitted_binary_round_trips_redirected_standard_streams
native-executable-stdin:staged_runtime_kit_emitted_binary_reads_redirected_standard_input
native-executable-stdout:staged_runtime_kit_emitted_binary_writes_redirected_standard_output
native-executable-stderr:staged_runtime_kit_emitted_binary_writes_redirected_standard_error
native-executable-args:staged_runtime_kit_emitted_binary_handoffs_core_args_through_the_executable_host
native-executable-unit-entry:staged_runtime_kit_emitted_binary_runs_a_custom_unit_entry
EOF
}

(
  cd "${fixture_root}"
  PATH="${fixture_root}/bin:${PATH}" \
  BESKID_RUNTIME_PREFIX="${prefix}" \
  BESKID_MATRIX_CARGO_CWDS="${fixture_root}/cargo-cwds" \
  BESKID_MATRIX_CARGO_CALLS="${fixture_root}/cargo-calls" \
  BESKID_MATRIX_NM_CALLS="${fixture_root}/nm-calls" \
  BESKID_MATRIX_READOBJ_CALLS="${fixture_root}/readobj-calls" \
  BESKID_MATRIX_UNAME_S="Darwin" \
  BESKID_MATRIX_UNAME_M="arm64" \
  BESKID_MATRIX_TARGET="aarch64-apple-darwin" \
  BESKID_MATRIX_STATIC_NAME="libbeskid_runtime.a" \
  BESKID_MATRIX_SHARED_NAME="libbeskid_runtime.dylib" \
  BESKID_MATRIX_CAPTURE_DIR="${fixture_root}/captured-symbols" \
    "${compiler_root}/scripts/stage-native-runtime-kit-matrix.sh" >/dev/null
)

test "$(sort -u "${fixture_root}/cargo-cwds")" = "${compiler_root}"
for profile in debug release; do
  for linkage in static shared; do
    symbols="${fixture_root}/captured-symbols/${profile}-${linkage}.symbols"
    test -f "${symbols}"
    grep -Fx 'target=aarch64-apple-darwin' "${symbols}" >/dev/null
    grep -Fx 'defined=beskid_rt_v5_entrypoint' "${symbols}" >/dev/null
    grep -Fx 'undefined=clock_gettime' "${symbols}" >/dev/null
  done
done
test "$(grep -c -- '--defined-only' "${fixture_root}/nm-calls")" = 4
test "$(grep -c -- '--undefined-only' "${fixture_root}/nm-calls")" = 4
grep -F -- '--debug-static-provenance-symbol-list' "${fixture_root}/cargo-calls" >/dev/null
grep -F -- '--debug-shared-provenance-symbol-list' "${fixture_root}/cargo-calls" >/dev/null
grep -F -- '--release-static-provenance-symbol-list' "${fixture_root}/cargo-calls" >/dev/null
grep -F -- '--release-shared-provenance-symbol-list' "${fixture_root}/cargo-calls" >/dev/null
if grep -E -- '--(debug|release)-provenance-symbol-list' "${fixture_root}/cargo-calls" >/dev/null; then
  echo 'legacy combined provenance option remains in the matrix invocation' >&2
  exit 1
fi
test "$(grep -Fc -- '-p beskid_engine --test engine native_runtime_kit_smoke::staged_runtime_kit_executes_a_canonical_entrypoint -- --exact --include-ignored' "${fixture_root}/cargo-calls")" = 2
test "$(grep -Fc -- '-p beskid_aot --test aot abi_v5_runtime_kit::staged_runtime_kit_links_and_executes_with_the_canonical_static_archive -- --exact --include-ignored' "${fixture_root}/cargo-calls")" = 2
test "$(grep -Fc -- '-p beskid_repl eval::tests::staged_native_runtime_kit_evaluates_a_snippet -- --exact --include-ignored' "${fixture_root}/cargo-calls")" = 2
test "$(grep -Fc -- 'run -q -p beskid_cli -- run ' "${fixture_root}/cargo-calls")" = 1
assert_lifecycle_smokes

rm -rf "${prefix}" "${fixture_root}/captured-symbols"
mkdir -p "${fixture_root}/captured-symbols"
: >"${fixture_root}/cargo-calls"
: >"${fixture_root}/nm-calls"
: >"${fixture_root}/readobj-calls"
(
  cd "${fixture_root}"
  PATH="${fixture_root}/bin:${PATH}" \
  BESKID_RUNTIME_PREFIX="${prefix}" \
  BESKID_MATRIX_CARGO_CWDS="${fixture_root}/cargo-cwds" \
  BESKID_MATRIX_CARGO_CALLS="${fixture_root}/cargo-calls" \
  BESKID_MATRIX_NM_CALLS="${fixture_root}/nm-calls" \
  BESKID_MATRIX_READOBJ_CALLS="${fixture_root}/readobj-calls" \
  BESKID_MATRIX_UNAME_S="MINGW64_NT-10.0" \
  BESKID_MATRIX_UNAME_M="x86_64" \
  BESKID_MATRIX_TARGET="x86_64-pc-windows-msvc" \
  BESKID_MATRIX_STATIC_NAME="beskid_runtime.lib" \
  BESKID_MATRIX_SHARED_NAME="beskid_runtime.dll" \
  BESKID_MATRIX_CAPTURE_DIR="${fixture_root}/captured-symbols" \
    "${compiler_root}/scripts/stage-native-runtime-kit-matrix.sh" >/dev/null
)
for profile in debug release; do
  static_symbols="${fixture_root}/captured-symbols/${profile}-static.symbols"
  shared_symbols="${fixture_root}/captured-symbols/${profile}-shared.symbols"
  grep -Fx 'target=x86_64-pc-windows-msvc' "${shared_symbols}" >/dev/null
  grep -Fx 'defined=beskid_rt_v5_entrypoint' "${shared_symbols}" >/dev/null
  grep -Fx 'undefined=TlsAlloc' "${shared_symbols}" >/dev/null
  grep -Fx 'defined=beskid_rt_v5_entrypoint' "${static_symbols}" >/dev/null
done
test "$(grep -c -- '--coff-exports' "${fixture_root}/readobj-calls")" = 2
test "$(grep -c -- '--coff-imports' "${fixture_root}/readobj-calls")" = 2
assert_lifecycle_smokes
if cat "${fixture_root}"/captured-symbols/*-shared.symbols | grep -F 'nm_dll_symbol' >/dev/null; then
  echo 'Windows DLL provenance still uses llvm-nm' >&2
  exit 1
fi

rm -rf "${prefix}" "${fixture_root}/captured-symbols"
mkdir -p "${fixture_root}/captured-symbols"
: >"${fixture_root}/cargo-calls"
: >"${fixture_root}/nm-calls"
: >"${fixture_root}/readobj-calls"
(
  cd "${fixture_root}"
  PATH="${fixture_root}/bin:${PATH}" \
  BESKID_RUNTIME_PREFIX="${prefix}" \
  BESKID_MATRIX_CARGO_CWDS="${fixture_root}/cargo-cwds" \
  BESKID_MATRIX_CARGO_CALLS="${fixture_root}/cargo-calls" \
  BESKID_MATRIX_NM_CALLS="${fixture_root}/nm-calls" \
  BESKID_MATRIX_READOBJ_CALLS="${fixture_root}/readobj-calls" \
  BESKID_MATRIX_UNAME_S="Linux" \
  BESKID_MATRIX_UNAME_M="x86_64" \
  BESKID_MATRIX_TARGET="x86_64-unknown-linux-gnu" \
  BESKID_MATRIX_STATIC_NAME="libbeskid_runtime.a" \
  BESKID_MATRIX_SHARED_NAME="libbeskid_runtime.so" \
  BESKID_MATRIX_CAPTURE_DIR="${fixture_root}/captured-symbols" \
    "${compiler_root}/scripts/stage-native-runtime-kit-matrix.sh" >/dev/null
)
for profile in debug release; do
  for linkage in static shared; do
    symbols="${fixture_root}/captured-symbols/${profile}-${linkage}.symbols"
    grep -Fx 'target=x86_64-unknown-linux-gnu' "${symbols}" >/dev/null
    grep -Fx 'defined=beskid_rt_v5_entrypoint' "${symbols}" >/dev/null
    grep -Fx 'undefined=clock_gettime' "${symbols}" >/dev/null
  done
done
assert_lifecycle_smokes

# A smoke whose filter selects no test (cargo reports "0 passed" and exits 0) fails the matrix.
rm -rf "${prefix}" "${fixture_root}/captured-symbols"
mkdir -p "${fixture_root}/captured-symbols"
if (
  cd "${fixture_root}"
  PATH="${fixture_root}/bin:${PATH}" \
  BESKID_RUNTIME_PREFIX="${prefix}" \
  BESKID_MATRIX_TESTS_RUN=0 \
  BESKID_MATRIX_CARGO_CWDS="${fixture_root}/cargo-cwds" \
  BESKID_MATRIX_CARGO_CALLS="${fixture_root}/cargo-calls" \
  BESKID_MATRIX_NM_CALLS="${fixture_root}/nm-calls" \
  BESKID_MATRIX_READOBJ_CALLS="${fixture_root}/readobj-calls" \
  BESKID_MATRIX_UNAME_S="Darwin" \
  BESKID_MATRIX_UNAME_M="arm64" \
  BESKID_MATRIX_TARGET="aarch64-apple-darwin" \
  BESKID_MATRIX_STATIC_NAME="libbeskid_runtime.a" \
  BESKID_MATRIX_SHARED_NAME="libbeskid_runtime.dylib" \
  BESKID_MATRIX_CAPTURE_DIR="${fixture_root}/captured-symbols" \
    "${compiler_root}/scripts/stage-native-runtime-kit-matrix.sh" >"${fixture_root}/zero-tests.log" 2>&1
); then
  echo 'a smoke that ran no test passed the matrix' >&2
  exit 1
fi
grep -F 'ran 0 tests; expected exactly 1' "${fixture_root}/zero-tests.log" >/dev/null
echo "native runtime-kit matrix workspace test passed"
