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
  while (($#)); do
    case "$1" in
      --prefix) prefix="$2"; shift 2 ;;
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
  elif [[ "${all_args}" == *" -- run "* ]]; then
    exit 42
  fi
fi
EOF
chmod +x "${fixture_root}/bin/cargo"

cat >"${fixture_root}/bin/llvm-nm" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"${BESKID_MATRIX_NM_CALLS}"
if [[ " $* " == *" .dll " ]]; then
  exit 0
fi
if [[ " $* " == *" --undefined-only "* ]]; then
  if [[ "${BESKID_MATRIX_TARGET}" == "x86_64-pc-windows-msvc" ]]; then
    printf 'TlsAlloc U 0\n'
  else
    printf '_clock_gettime U 0\n'
  fi
else
  if [[ "${BESKID_MATRIX_TARGET}" == "x86_64-pc-windows-msvc" ]]; then
    printf 'beskid_rt_v5_entrypoint T 0\n'
  else
    printf '_beskid_rt_v5_entrypoint T 0\n'
  fi
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
test "$(grep -Fc -- '-p beskid_engine --test native_runtime_kit_smoke staged_runtime_kit_executes_a_canonical_entrypoint -- --ignored --exact' "${fixture_root}/cargo-calls")" = 2
test "$(grep -Fc -- '-p beskid_aot --test abi_v5_runtime_kit staged_runtime_kit_links_and_executes_with_the_canonical_static_archive -- --ignored --exact' "${fixture_root}/cargo-calls")" = 2
test "$(grep -Fc -- '-p beskid_repl eval::tests::staged_native_runtime_kit_evaluates_a_snippet -- --ignored --exact' "${fixture_root}/cargo-calls")" = 2
test "$(grep -Fc -- 'run -q -p beskid_cli -- run ' "${fixture_root}/cargo-calls")" = 1

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
if grep -F -- '.dll' "${fixture_root}/nm-calls" >/dev/null; then
  echo 'Windows DLL provenance still uses llvm-nm' >&2
  exit 1
fi
echo "native runtime-kit matrix workspace test passed"
