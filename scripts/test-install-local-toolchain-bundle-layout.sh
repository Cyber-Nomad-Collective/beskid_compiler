#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
test_root="$(mktemp -d)"
trap 'rm -rf "${test_root}"' EXIT

bundle="${test_root}/bundle"
prefix="${test_root}/prefix"
mkdir -p "${bundle}/bin" "${bundle}/lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/debug" \
  "${bundle}/lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/release" \
  "${bundle}/beskid_corelib/beskid_corelib" \
  "${bundle}/beskid_corelib/packages/compiler-sdk"

cat >"${bundle}/bin/beskid" <<'EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == dev && "${2:-}" == toolchain-owner ]]; then
  script_root="$(cd "$(dirname "$0")/.." && pwd)"
  node "${BESKID_TEST_OWNER_STAMPER:?}" "${script_root}" manual 0.6.0 "${BESKID_TEST_OWNER_TARGET:?}"
elif [[ "${1:-}" == toolchain ]]; then
  printf '%s\n' 'Installation owner: manual'
else
  printf '%s\n' 'beskid 0.6.0'
fi
EOF
cat >"${bundle}/bin/beskid_lsp" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'beskid_lsp 0.6.0'
EOF
cat >"${bundle}/bin/beskid-up" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'beskid-up 0.6.0'
EOF
chmod +x "${bundle}/bin/"*

touch "${bundle}/beskid_corelib/CoreLib.bws"
touch "${bundle}/beskid_corelib/beskid_corelib/corelib.bproj"
touch "${bundle}/beskid_corelib/.beskid-bundle.sha256"
printf '%s\n' '0.6.0' >"${bundle}/release-version.txt"
printf '%s\n' 'compiler sdk package' \
  >"${bundle}/beskid_corelib/packages/compiler-sdk/package.marker"

export BESKID_TEST_OWNER_STAMPER="${script_dir}/../../beskid_distrib/scripts/stamp-installation-owner.mjs"
export BESKID_TEST_OWNER_TARGET=x86_64-unknown-linux-gnu
mkdir -p "${prefix}/config"
printf 'user config' >"${prefix}/config/settings"
bash "${script_dir}/install-local-toolchain-bundle.sh" "${bundle}" 0.6.0 "${prefix}"
[[ -f "${prefix}/toolchain/beskid_corelib/packages/compiler-sdk/package.marker" ]]
[[ "$("${prefix}/bin/beskid" --version)" == 'beskid 0.6.0' ]]
[[ "$(cat "${prefix}/config/settings")" == 'user config' ]]
node - "${prefix}/toolchain/.beskid-owner.json" <<'NODE'
const receipt = require(process.argv[2]);
if (receipt.owner !== 'manual' || Object.keys(receipt.files).some(path => path.startsWith('config/'))) process.exit(1);
NODE

windows_bundle="${test_root}/windows-bundle"
windows_prefix="${test_root}/windows-prefix"
cp -a "${bundle}" "${windows_bundle}"
mv "${windows_bundle}/lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu" \
  "${windows_bundle}/lib/beskid-runtime/abi-5/x86_64-pc-windows-msvc"
for binary in beskid beskid_lsp beskid-up; do
  mv "${windows_bundle}/bin/${binary}" "${windows_bundle}/bin/${binary}.exe"
done
export BESKID_TEST_OWNER_TARGET=x86_64-pc-windows-msvc
bash "${script_dir}/install-local-toolchain-bundle.sh" \
  "${windows_bundle}" 0.6.0 "${windows_prefix}"
for binary in beskid beskid_lsp beskid-up; do
  [[ -x "${windows_prefix}/toolchain/bin/${binary}.exe" ]]
  [[ -f "${windows_prefix}/bin/${binary}.cmd" ]]
done
export BESKID_TEST_OWNER_TARGET=x86_64-unknown-linux-gnu
conflict="${test_root}/conflict"
mkdir -p "${conflict}/toolchain"
printf 'user-owned collision' >"${conflict}/toolchain/keep"
if bash "${script_dir}/install-local-toolchain-bundle.sh" "${bundle}" 0.6.0 "${conflict}"; then
  echo 'Installer accepted an unowned private-prefix collision' >&2; exit 1
fi
[[ "$(cat "${conflict}/toolchain/keep")" == 'user-owned collision' ]]
