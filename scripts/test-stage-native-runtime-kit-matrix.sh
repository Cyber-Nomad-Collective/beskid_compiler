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
if [[ "$1" == "run" ]]; then
  prefix=""
  profile=""
  while (($#)); do
    case "$1" in
      --prefix) prefix="$2"; shift 2 ;;
      --profile) profile="$2"; shift 2 ;;
      *) shift ;;
    esac
  done
  if [[ -n "${prefix}" && -n "${profile}" ]]; then
    target="aarch64-apple-darwin"
    root="${prefix}/lib/beskid-runtime/abi-5/${target}/${profile}"
    mkdir -p "${root}/static" "${root}/shared"
    : >"${root}/static/libbeskid_runtime.a"
    : >"${root}/shared/libbeskid_runtime.dylib"
  fi
fi
EOF
chmod +x "${fixture_root}/bin/cargo"

cat >"${fixture_root}/bin/llvm-nm" <<'EOF'
#!/usr/bin/env bash
printf 'beskid_rt_v5_entrypoint T 0\n'
EOF
chmod +x "${fixture_root}/bin/llvm-nm"

prefix="${fixture_root}/prefix"
(
  cd "${fixture_root}"
  PATH="${fixture_root}/bin:${PATH}" \
  BESKID_RUNTIME_PREFIX="${prefix}" \
  BESKID_MATRIX_CARGO_CWDS="${fixture_root}/cargo-cwds" \
    "${compiler_root}/scripts/stage-native-runtime-kit-matrix.sh" >/dev/null
)

test "$(sort -u "${fixture_root}/cargo-cwds")" = "${compiler_root}"
echo "native runtime-kit matrix workspace test passed"
