#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
test_root="$(mktemp -d)"
trap 'rm -rf "${test_root}"' EXIT

bundle="${test_root}/bundle"
prefix="${test_root}/prefix"
mkdir -p "${bundle}/bin" "${bundle}/lib/beskid-runtime/abi-5" \
  "${bundle}/beskid_corelib/beskid_corelib" \
  "${bundle}/beskid_corelib/packages/compiler-sdk"

cat >"${bundle}/bin/beskid" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'beskid 0.5.0'
EOF
cat >"${bundle}/bin/beskid_lsp" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'beskid_lsp 0.5.0'
EOF
cat >"${bundle}/bin/beskid-up" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'beskid-up 0.5.0'
EOF
chmod +x "${bundle}/bin/"*

touch "${bundle}/beskid_corelib/CoreLib.bws"
touch "${bundle}/beskid_corelib/beskid_corelib/corelib.bproj"
touch "${bundle}/beskid_corelib/.beskid-bundle.sha256"
printf '%s\n' '0.5.0' >"${bundle}/release-version.txt"
printf '%s\n' 'compiler sdk package' \
  >"${bundle}/beskid_corelib/packages/compiler-sdk/package.marker"

bash "${script_dir}/install-local-toolchain-bundle.sh" "${bundle}" 0.5.0 "${prefix}"
[[ -f "${prefix}/beskid_corelib/packages/compiler-sdk/package.marker" ]]

windows_bundle="${test_root}/windows-bundle"
windows_prefix="${test_root}/windows-prefix"
cp -a "${bundle}" "${windows_bundle}"
for binary in beskid beskid_lsp beskid-up; do
  mv "${windows_bundle}/bin/${binary}" "${windows_bundle}/bin/${binary}.exe"
done
bash "${script_dir}/install-local-toolchain-bundle.sh" \
  "${windows_bundle}" 0.5.0 "${windows_prefix}"
[[ -x "${windows_prefix}/bin/beskid.exe" ]]
[[ -x "${windows_prefix}/bin/beskid_lsp.exe" ]]
[[ -x "${windows_prefix}/bin/beskid-up.exe" ]]
