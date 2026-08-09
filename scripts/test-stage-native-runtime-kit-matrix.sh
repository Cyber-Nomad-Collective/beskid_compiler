#!/usr/bin/env bash
# Regression test: the matrix script must run Cargo from the compiler workspace,
# even when GitHub Actions invokes it from the superproject checkout.
set -euo pipefail

compiler_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
trap 'rm -rf "${fixture_root}"' EXIT

mkdir -p "${fixture_root}/bin"
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
    target="aarch64-apple-darwin"
    root="${prefix}/lib/beskid-runtime/abi-5/${target}/${profile}"
    mkdir -p "${root}/static" "${root}/shared"
    : >"${root}/static/libbeskid_runtime.a"
    : >"${root}/shared/libbeskid_runtime.dylib"
  elif [[ "${all_args}" == *" runtime-kit build-matrix "* ]]; then
    test -n "${debug_static_symbols}"
    test -n "${debug_shared_symbols}"
    test -n "${release_static_symbols}"
    test -n "${release_shared_symbols}"
    cp "${debug_static_symbols}" "${BESKID_MATRIX_CAPTURE_DIR}/debug-static.symbols"
    cp "${debug_shared_symbols}" "${BESKID_MATRIX_CAPTURE_DIR}/debug-shared.symbols"
    cp "${release_static_symbols}" "${BESKID_MATRIX_CAPTURE_DIR}/release-static.symbols"
    cp "${release_shared_symbols}" "${BESKID_MATRIX_CAPTURE_DIR}/release-shared.symbols"
  fi
fi
EOF
chmod +x "${fixture_root}/bin/cargo"

cat >"${fixture_root}/bin/llvm-nm" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"${BESKID_MATRIX_NM_CALLS}"
if [[ " $* " == *" --undefined-only "* ]]; then
  printf '_clock_gettime U 0\n'
else
  printf '_beskid_rt_v5_entrypoint T 0\n'
fi
EOF
chmod +x "${fixture_root}/bin/llvm-nm"

prefix="${fixture_root}/prefix"
mkdir -p "${fixture_root}/captured-symbols"
(
  cd "${fixture_root}"
  PATH="${fixture_root}/bin:${PATH}" \
  BESKID_RUNTIME_PREFIX="${prefix}" \
  BESKID_MATRIX_CARGO_CWDS="${fixture_root}/cargo-cwds" \
  BESKID_MATRIX_CARGO_CALLS="${fixture_root}/cargo-calls" \
  BESKID_MATRIX_NM_CALLS="${fixture_root}/nm-calls" \
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
echo "native runtime-kit matrix workspace test passed"
