#!/usr/bin/env bash
# Build beskid_runtime_bridge when the prebuilt static library is missing.
#
# AOT tests and AotBuildRequest::with_defaults() resolve the archive from
# target/{profile}/libbeskid_runtime_bridge.a (or a per-triple subdirectory).
# CI runs this before workspace tests; local `just compiler` and beskid_aot's
# build.rs call it automatically.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

profile="${BESKID_RUNTIME_BRIDGE_PROFILE:-debug}"
case "$profile" in
  debug | release) ;;
  *)
    echo "Unsupported BESKID_RUNTIME_BRIDGE_PROFILE: ${profile}" >&2
    exit 1
    ;;
esac

if [[ "${OS:-}" == "Windows_NT" ]] || [[ "$(uname -s 2>/dev/null || echo)" == MINGW* ]]; then
  lib_name="beskid_runtime_bridge.lib"
else
  lib_name="libbeskid_runtime_bridge.a"
fi

target_root="${CARGO_TARGET_DIR:-$ROOT/target}"
candidates=("$target_root/$profile/$lib_name")

if [[ -n "${HOST:-}" ]]; then
  candidates+=("$target_root/$HOST/$profile/$lib_name")
fi
if [[ -n "${CARGO_BUILD_TARGET:-}" ]]; then
  candidates+=("$target_root/$CARGO_BUILD_TARGET/$profile/$lib_name")
fi

for candidate in "${candidates[@]}"; do
  if [[ -f "$candidate" ]]; then
    exit 0
  fi
done

build_args=(build -p beskid_runtime_bridge)
if [[ "$profile" == "release" ]]; then
  build_args+=(--release)
fi
if [[ -n "${CARGO_BUILD_TARGET:-}" ]]; then
  build_args+=(--target "$CARGO_BUILD_TARGET")
fi

echo "==> Building beskid_runtime_bridge (${profile}) for AOT tests"
cargo "${build_args[@]}"
