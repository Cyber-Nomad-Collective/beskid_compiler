#!/usr/bin/env bash
# Regression test for crashdiag.sh's signal_meaning() mapping -- the pure signal-code-to-name/note
# lookup, exercised without ssh/builder access. We extract just that function's body from the
# script (rather than sourcing/running the whole script, which parses argv and would `exit 2`
# with no builder inputs) and eval it in this subshell.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

FUNC_SRC="$(sed -n '/^signal_meaning() {/,/^}/p' "$DIR/crashdiag.sh")"
if [ -z "$FUNC_SRC" ]; then
  echo "FAIL: could not extract signal_meaning() from crashdiag.sh" >&2
  exit 1
fi
eval "$FUNC_SRC"

fail=0
check() {
  local code="$1" expect_signal="$2"
  local out signal
  out="$(signal_meaning "$code")"
  signal="$(echo "$out" | cut -d'|' -f1)"
  if [ "$signal" != "$expect_signal" ]; then
    echo "FAIL: signal_meaning($code) -> signal='$signal', expected '$expect_signal'" >&2
    fail=1
  else
    echo "OK: signal_meaning($code) -> $signal"
  fi
}

check 132 SIGILL
check 139 SIGSEGV
check 134 SIGABRT
check 137 SIGKILL
check 133 SIGTRAP
check 136 SIGFPE
check 138 SIGBUS
check 0 unknown
check 999 unknown

# The note half must be non-empty for every recognized code (a blank note would silently lose the
# codebase-specific context the tool exists to provide).
for code in 132 139 134 137 133 136 138; do
  note="$(signal_meaning "$code" | cut -d'|' -f2)"
  if [ -z "$note" ]; then
    echo "FAIL: signal_meaning($code) has an empty note" >&2
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "All signal_meaning() checks passed."
