#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
matrix="${root}/scripts/stage-native-runtime-kit-matrix.sh"

if grep -Fq 'Testing.Assert' "${matrix}"; then
  echo 'runtime-kit CLI smoke depends on a package outside the installed runtime kit' >&2
  exit 1
fi
grep -Fq 'i64 semanticValue = 6 * 7;' "${matrix}" || {
  echo 'runtime-kit CLI smoke does not compute its semantic result in self-contained source' >&2
  exit 1
}
grep -Fq 'expected 42' "${matrix}" || {
  echo 'runtime-kit CLI smoke does not validate the program exit result' >&2
  exit 1
}
grep -Fq 'smoke "${target}" debug cli shared passed "${smoke_status}"' "${matrix}" || {
  echo 'runtime-kit CLI smoke does not retain successful semantic-result evidence' >&2
  exit 1
}

echo 'native runtime-kit CLI smoke contract test passed'
