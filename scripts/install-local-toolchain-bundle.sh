#!/usr/bin/env bash
# Replace one local Beskid prefix from an already verified release bundle.
set -euo pipefail

bundle_source="${1:?bundle directory}"
release_version="${2:?release version}"
requested_prefix="${3:?install prefix}"

[[ -d "${bundle_source}" ]] || { echo "Missing local bundle: ${bundle_source}" >&2; exit 1; }
bundle_source="$(cd "${bundle_source}" && pwd)"

prefix_parent="$(dirname "${requested_prefix}")"
prefix_name="$(basename "${requested_prefix}")"
[[ "${prefix_name}" != "." && "${prefix_name}" != ".." && -n "${prefix_name}" ]] || {
  echo "Unsafe local toolchain prefix: ${requested_prefix}" >&2
  exit 1
}
mkdir -p "${prefix_parent}"
prefix_parent="$(cd "${prefix_parent}" && pwd)"
install_prefix="${prefix_parent}/${prefix_name}"
[[ "${install_prefix}" != "/" ]] || { echo 'Refusing to replace the filesystem root.' >&2; exit 1; }
[[ ! -L "${install_prefix}" ]] || { echo "Refusing to replace symlinked prefix: ${install_prefix}" >&2; exit 1; }
[[ ! -e "${install_prefix}" || -d "${install_prefix}" ]] || {
  echo "Local toolchain prefix is not a directory: ${install_prefix}" >&2
  exit 1
}

for binary in beskid beskid_lsp beskid-up; do
  [[ -f "${bundle_source}/bin/${binary}" ]] || {
    echo "Local bundle omitted bin/${binary}" >&2
    exit 1
  }
done
[[ -d "${bundle_source}/lib/beskid-runtime/abi-5" ]] || {
  echo 'Local bundle omitted its ABI-v5 runtime kit.' >&2
  exit 1
}
[[ -f "${bundle_source}/beskid_corelib/corelib.bproj" ]] || {
  echo 'Local bundle omitted corelib.' >&2
  exit 1
}
[[ -d "${bundle_source}/packages" ]] || { echo 'Local bundle omitted bundled packages.' >&2; exit 1; }
[[ -f "${bundle_source}/release-version.txt" ]] || { echo 'Local bundle omitted release-version.txt.' >&2; exit 1; }
printf '%s\n' "${release_version}" | cmp -s - "${bundle_source}/release-version.txt" || {
  echo 'Local bundle release-version.txt does not match the requested version exactly.' >&2
  exit 1
}

incoming="$(mktemp -d "${prefix_parent}/.${prefix_name}.install.XXXXXX")"
backup=""
cleanup() {
  status=$?
  if [[ "${status}" -ne 0 && -n "${backup}" && -e "${backup}" && ! -e "${install_prefix}" ]]; then
    mv "${backup}" "${install_prefix}" || \
      echo "Failed to restore prior toolchain from ${backup}" >&2
  fi
  [[ -z "${incoming}" || ! -e "${incoming}" ]] || rm -rf "${incoming}"
  [[ "${status}" -ne 0 || -z "${backup}" || ! -e "${backup}" ]] || rm -rf "${backup}"
  return "${status}"
}
trap cleanup EXIT

cp -a "${bundle_source}/." "${incoming}/"
chmod 0755 "${incoming}/bin/beskid" "${incoming}/bin/beskid_lsp" "${incoming}/bin/beskid-up"

# The prefix also stores user-owned shell data and configuration. Carry those entries into the
# staged tree, while the bundle remains the sole authority for every release-owned entry.
if [[ -d "${install_prefix}" ]]; then
  shopt -s dotglob nullglob
  for existing_entry in "${install_prefix}"/*; do
    entry_name="$(basename "${existing_entry}")"
    case "${entry_name}" in
      bin|lib|beskid_corelib|packages|release-version.txt) continue ;;
    esac
    [[ -e "${incoming}/${entry_name}" || -L "${incoming}/${entry_name}" ]] || \
      cp -a "${existing_entry}" "${incoming}/${entry_name}"
  done
  shopt -u dotglob nullglob
fi

[[ "$("${incoming}/bin/beskid" --version 2>&1)" == "beskid ${release_version}" ]] || {
  echo 'CLI version does not match the local bundle version.' >&2
  exit 1
}
[[ "$("${incoming}/bin/beskid_lsp" --version 2>&1)" == "beskid_lsp ${release_version}" ]] || {
  echo 'LSP version does not match the local bundle version.' >&2
  exit 1
}
[[ "$("${incoming}/bin/beskid-up" --version 2>&1)" == "beskid-up ${release_version}" ]] || {
  echo 'Updater version does not match the local bundle version.' >&2
  exit 1
}

if [[ -e "${install_prefix}" ]]; then
  backup="${prefix_parent}/.${prefix_name}.backup.$$"
  [[ ! -e "${backup}" ]] || { echo "Local toolchain backup already exists: ${backup}" >&2; exit 1; }
  mv "${install_prefix}" "${backup}"
fi

if ! mv "${incoming}" "${install_prefix}"; then
  echo "Failed to publish staged local toolchain at ${install_prefix}" >&2
  exit 1
fi
incoming=""

if [[ -n "${backup}" ]]; then
  rm -rf "${backup}"
  backup=""
fi

echo "Installed Beskid ${release_version} bundle under ${install_prefix}"
