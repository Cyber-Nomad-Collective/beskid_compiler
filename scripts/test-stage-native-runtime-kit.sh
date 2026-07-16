#!/usr/bin/env bash
# Regression test for the CI runtime-kit staging wrapper without invoking a compiler build.
set -euo pipefail

compiler_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
trap 'rm -rf "${fixture_root}"' EXIT

prefix="${fixture_root}/prefix"
mkdir -p "${prefix}/lib/beskid-runtime/abi-5/stale"
printf '%s\n' stale > "${prefix}/lib/beskid-runtime/abi-5/stale/abi.json"
fake_cli="${fixture_root}/beskid"
cat > "${fake_cli}" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "${BESKID_RUNTIME_PREFIX}" > "${BESKID_STAGE_LOG}"
printf '%s\n' "$*" >> "${BESKID_STAGE_LOG}"
EOF
chmod +x "${fake_cli}"

BESKID_RUNTIME_PREFIX="${prefix}" \
BESKID_RUNTIME_KIT_PROFILE=release \
BESKID_CLI_BIN="${fake_cli}" \
BESKID_STAGE_LOG="${fixture_root}/stage.log" \
  "${compiler_root}/scripts/stage-native-runtime-kit.sh" >/dev/null

test ! -e "${prefix}/lib/beskid-runtime/abi-5/stale"
grep -Fx "${prefix}" "${fixture_root}/stage.log" >/dev/null
grep -Fx "runtime-kit build-native-host --prefix ${prefix} --profile release" \
  "${fixture_root}/stage.log" >/dev/null

echo "native runtime-kit staging script test passed"
