#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
project_root="${repo_root}/runtime/beskid/tests/runtime_semantics"
cli="${BESKID_CLI:-}"

if [[ "$(uname -s)" != Linux ]]; then
  echo "this deterministic getaddrinfo interposer test is Linux-only" >&2
  exit 2
fi
if [[ -z "${cli}" || ! -x "${cli}" ]]; then
  echo "set BESKID_CLI to an executable beskid_cli built from this checkout" >&2
  exit 2
fi
for variable in BESKID_RUNTIME_PREFIX BESKID_CORELIB_ROOT; do
  if [[ -z "${!variable:-}" ]]; then
    echo "set ${variable} to the exact runtime kit/corelib source used by this test" >&2
    exit 2
  fi
done

temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/beskid-dns-deadline.XXXXXX")"
shim="${temp_dir}/libbeskid_dns_deadline.so"
cleanup() {
  rm -f "${shim}"
  rmdir "${temp_dir}"
}
trap cleanup EXIT
cc -std=c11 -O2 -Wall -Wextra -Werror -fPIC -shared \
  "${repo_root}/scripts/diagnose/dns-deadline-resolver-shim.c" \
  -o "${shim}" -ldl -pthread

cd "${repo_root}"
BESKID_DNS_DEADLINE_SHIM=1 \
LD_PRELOAD="${shim}${LD_PRELOAD:+:${LD_PRELOAD}}" \
  "${cli}" test --plain --project "${project_root}" --target NetworkNativeTests --target-timeout 900
