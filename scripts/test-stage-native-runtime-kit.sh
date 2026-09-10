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

mkdir -p "${fixture_root}/bin"
cat > "${fixture_root}/bin/uname" <<'EOF'
#!/usr/bin/env bash
case "${1:-}" in
  -s) printf '%s\n' "${BESKID_TEST_UNAME_S:-Linux}" ;;
  -m) printf '%s\n' x86_64 ;;
  *) exit 1 ;;
esac
EOF
chmod +x "${fixture_root}/bin/uname"

PATH="${fixture_root}/bin:${PATH}" \
BESKID_RUNTIME_PREFIX="${prefix}" \
BESKID_RUNTIME_KIT_PROFILE=release \
BESKID_CLI_BIN="${fake_cli}" \
BESKID_STAGE_LOG="${fixture_root}/stage.log" \
  "${compiler_root}/scripts/stage-native-runtime-kit.sh" build >/dev/null

test ! -e "${prefix}/lib/beskid-runtime/abi-5/stale"
grep -Fx "${prefix}" "${fixture_root}/stage.log" >/dev/null
grep -Fx "runtime-kit build-native-host --prefix ${prefix} --profile release" \
  "${fixture_root}/stage.log" >/dev/null

default_prefix="${fixture_root}/default-prefix"
PATH="${fixture_root}/bin:${PATH}" \
BESKID_TEST_UNAME_S=Darwin \
BESKID_RUNTIME_PREFIX="${default_prefix}" \
BESKID_CLI_BIN="${fake_cli}" \
BESKID_STAGE_LOG="${fixture_root}/default-stage.log" \
  "${compiler_root}/scripts/stage-native-runtime-kit.sh" >/dev/null
grep -Fx "runtime-kit build-native-host --prefix ${default_prefix} --profile debug" \
  "${fixture_root}/default-stage.log" >/dev/null

set +e
"${compiler_root}/scripts/stage-native-runtime-kit.sh" unsupported >"${fixture_root}/unsupported.log" 2>&1
unsupported_status=$?
set -e
test "${unsupported_status}" -eq 2
grep -F 'unsupported native runtime-kit phase: unsupported' "${fixture_root}/unsupported.log" >/dev/null

echo "native runtime-kit staging script test passed"
