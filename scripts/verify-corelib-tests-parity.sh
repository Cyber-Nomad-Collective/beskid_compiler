#!/usr/bin/env bash
# Ensure corelib_tests.bproj targets match spine typecheck gates.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BPROJ="${ROOT}/corelib/beskid_corelib/tests/corelib_tests/corelib_tests.bproj"
TYPECHECK="${ROOT}/crates/beskid_tests/src/spine/corelib_tests_typecheck.rs"

if [[ ! -f "${BPROJ}" ]]; then
  echo "missing ${BPROJ}" >&2
  exit 1
fi
if [[ ! -f "${TYPECHECK}" ]]; then
  echo "missing ${TYPECHECK}" >&2
  exit 1
fi

bproj_list="$(mktemp)"
spine_list="$(mktemp)"
trap 'rm -f "${bproj_list}" "${spine_list}"' EXIT

rg 'entry = "([^"]+)"' "${BPROJ}" -o -r '$1' | sort -u > "${bproj_list}"
rg '"[a-z]+/[^"]+\.bd"' "${TYPECHECK}" -o | tr -d '"' | sort -u > "${spine_list}"

missing_in_spine=0
while IFS= read -r entry; do
  if ! grep -qx "${entry}" "${spine_list}"; then
    echo "  in .bproj but not typecheck spine: ${entry}" >&2
    missing_in_spine=1
  fi
done < "${bproj_list}"

orphan_in_spine=0
while IFS= read -r entry; do
  if ! grep -qx "${entry}" "${bproj_list}"; then
    echo "  in typecheck spine but not .bproj: ${entry}" >&2
    orphan_in_spine=1
  fi
done < "${spine_list}"

if [[ "${missing_in_spine}" -ne 0 || "${orphan_in_spine}" -ne 0 ]]; then
  echo "corelib_tests parity drift detected" >&2
  exit 1
fi

count="$(wc -l < "${bproj_list}" | tr -d ' ')"
echo "corelib_tests parity ok (${count} targets)"
