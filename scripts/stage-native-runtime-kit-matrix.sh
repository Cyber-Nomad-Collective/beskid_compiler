#!/usr/bin/env bash
# Build the two native ABI-v5 profiles, then atomically publish them as one exact kit matrix.
#
# The temporary profile prefixes deliberately never become a consumer prefix.  Their artifacts
# are only inputs to `runtime-kit build-matrix`; the empty BESKID_RUNTIME_PREFIX below is the
# single installed coordinate exercised by JIT and AOT smoke tests.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target_root="${CARGO_TARGET_DIR:-${root}/target}"
prefix="${BESKID_RUNTIME_PREFIX:-${target_root}/native-runtime-kit-matrix}"
runtime_root="${prefix}/lib/beskid-runtime/abi-5"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)
    target="x86_64-unknown-linux-gnu"
    static_name="libbeskid_runtime.a"
    shared_name="libbeskid_runtime.so"
    ;;
  Darwin-arm64)
    target="aarch64-apple-darwin"
    static_name="libbeskid_runtime.a"
    shared_name="libbeskid_runtime.dylib"
    ;;
  MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64)
    target="x86_64-pc-windows-msvc"
    static_name="beskid_runtime.lib"
    shared_name="beskid_runtime.dll"
    ;;
  *)
    echo "Unsupported native ABI-v5 runtime-kit matrix host: $(uname -s)-$(uname -m)" >&2
    exit 1
    ;;
esac

if [[ -e "${runtime_root}/${target}" ]]; then
  echo "BESKID_RUNTIME_PREFIX must be an empty runtime-kit destination: ${prefix}" >&2
  exit 1
fi
mkdir -p "${prefix}"

if [[ -n "${BESKID_CLI_BIN:-}" ]]; then
  cli=("${BESKID_CLI_BIN}")
else
  cli=(cargo run -q -p beskid_cli --)
fi

work="$(mktemp -d "${TMPDIR:-/tmp}/beskid-native-runtime-kit-matrix.XXXXXX")"
trap 'rm -rf "${work}"' EXIT

stage_profile() {
  local profile="$1"
  local profile_prefix="${work}/${profile}-source"
  "${cli[@]}" runtime-kit build-native-host --prefix "${profile_prefix}" --profile "${profile}"
}

symbol_tool="${LLVM_NM:-}"
if [[ -z "${symbol_tool}" ]]; then
  symbol_tool="$(command -v llvm-nm || true)"
fi
if [[ -z "${symbol_tool}" ]]; then
  echo "Native runtime-kit provenance requires llvm-nm; set LLVM_NM to the native tool path" >&2
  exit 1
fi

write_provenance() {
  local profile="$1"
  local profile_prefix="${work}/${profile}-source"
  local profile_root="${profile_prefix}/lib/beskid-runtime/abi-5/${target}/${profile}"
  local symbols="${work}/${profile}.symbols"
  local static_library="${profile_root}/static/${static_name}"
  local shared_library="${profile_root}/shared/${shared_name}"

  [[ -f "${static_library}" ]] || { echo "Missing native static runtime: ${static_library}" >&2; exit 1; }
  [[ -f "${shared_library}" ]] || { echo "Missing native shared runtime: ${shared_library}" >&2; exit 1; }

  {
    printf 'target=%s\n' "${target}"
    "${symbol_tool}" --extern-only --defined-only --format=posix "${static_library}" "${shared_library}" \
      | awk 'NF >= 2 { print "defined=" $1 }'
    "${symbol_tool}" --extern-only --undefined-only --format=posix "${shared_library}" \
      | awk 'NF >= 2 { print "undefined=" $1 }'
  } >"${symbols}"

  # Mach-O spellings carry a leading underscore; ABI manifests do not.
  if [[ "${target}" == "aarch64-apple-darwin" ]]; then
    sed -i.bak -E 's/^(defined|undefined)=_/\1=/' "${symbols}"
    rm -f "${symbols}.bak"
  fi

  [[ -s "${symbols}" ]] || { echo "Empty provenance symbol report: ${symbols}" >&2; exit 1; }
}

stage_profile debug
stage_profile release
write_provenance debug
write_provenance release

debug_root="${work}/debug-source/lib/beskid-runtime/abi-5/${target}/debug"
release_root="${work}/release-source/lib/beskid-runtime/abi-5/${target}/release"
matrix_args=(
  runtime-kit build-matrix --prefix "${prefix}" --target "${target}"
  --debug-static-library "${debug_root}/static/${static_name}"
  --debug-shared-library "${debug_root}/shared/${shared_name}"
  --release-static-library "${release_root}/static/${static_name}"
  --release-shared-library "${release_root}/shared/${shared_name}"
  --debug-provenance-symbol-list "${work}/debug.symbols"
  --release-provenance-symbol-list "${work}/release.symbols"
)

if [[ "${target}" == "x86_64-pc-windows-msvc" ]]; then
  debug_import="${debug_root}/shared/beskid_runtime_import.lib"
  release_import="${release_root}/shared/beskid_runtime_import.lib"
  [[ -f "${debug_import}" && -f "${release_import}" ]] || {
    echo "Windows ABI-v5 runtime kit requires debug and release import libraries" >&2
    exit 1
  }
  matrix_args+=(
    --debug-shared-import-library "${debug_import}"
    --release-shared-import-library "${release_import}"
  )
fi

"${cli[@]}" "${matrix_args[@]}"

export BESKID_RUNTIME_PREFIX="${prefix}"
for profile in debug release; do
  export BESKID_RUNTIME_KIT_PROFILE="${profile}"
  cargo test -p beskid_engine --test native_runtime_kit_smoke \
    staged_runtime_kit_executes_a_canonical_entrypoint -- --ignored --exact
  cargo test -p beskid_aot --test abi_v5_runtime_kit \
    staged_runtime_kit_resolves_the_canonical_static_archive -- --ignored --exact
done

echo "Native ABI-v5 runtime-kit matrix evidence passed for ${target} at ${prefix}"
