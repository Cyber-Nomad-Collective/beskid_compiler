#!/usr/bin/env bash
# stalled.sh [N_MIN=20] — flag a log that looks IN PROGRESS (no "test result:"/"Tests
# failed"/"EXIT=" line yet) but hasn't been written to in > N_MIN minutes, with no
# cargo/beskid_cli process whose command line mentions that log path: a dead run nobody
# is waiting on. Prints the log and, when the run's own command is still visible in ps, enough
# to identify what should be restarted; otherwise just names the log so a human/agent can check
# what was supposed to write it.
#
# Configuration (env var, default in parens) -- same convention as crashdiag.sh:
#   BESKID_DIAG_JUMP_HOST    ssh jump host                          (root@bdziam.dev)
#   BESKID_DIAG_BUILDER_HOST ssh target host, reached via the jump   (root@10.66.0.2)
#   BESKID_DIAG_CONTAINER    podman container name on the builder    (beskid-codex-build)
#   BESKID_DIAG_LOG_DIR      builder-side per-slice log directory    (/var/lib/beskid-codex-build/run)
set -euo pipefail
N_MIN="${1:-20}"

JUMP_HOST="${BESKID_DIAG_JUMP_HOST:-root@bdziam.dev}"
BUILDER_HOST="${BESKID_DIAG_BUILDER_HOST:-root@10.66.0.2}"
CONTAINER="${BESKID_DIAG_CONTAINER:-beskid-codex-build}"
LOG_DIR="${BESKID_DIAG_LOG_DIR:-/var/lib/beskid-codex-build/run}"
SSH="ssh -o BatchMode=yes -J $JUMP_HOST $BUILDER_HOST"

REMOTE="
echo '===NOW==='; date +%s
echo '===PROCS==='
podman exec $CONTAINER ps -eo pid,cmd 2>/dev/null | grep -E 'cargo (build|test|check)|beskid_cli' | grep -v grep || true
echo '===LOGS==='
for f in $LOG_DIR/*.log; do
  [ -f \"\$f\" ] || continue
  m=\$(stat -c \"%Y\" \"\$f\" 2>/dev/null || stat -f \"%m\" \"\$f\")
  has_result=\$(grep -cE 'test result:|Tests failed|EXIT=' \"\$f\" || true)
  echo \"\$f \$m \$has_result\"
done
"
OUT=$($SSH "$REMOTE")
NOW=$(echo "$OUT" | sed -n '/===NOW===/,/===PROCS===/p' | sed -n '2p')
PROCS=$(echo "$OUT" | sed -n '/===PROCS===/,/===LOGS===/p' | sed '1d;$d')
LOGS=$(echo "$OUT" | sed -n '/===LOGS===/,$p' | sed '1d')

THRESH=$((N_MIN * 60))
FOUND=0
while IFS=' ' read -r path mtime has_result; do
  [ -z "$path" ] && continue
  [ "${has_result:-0}" -gt 0 ] && continue   # already finished, not a dead run
  age=$((NOW - mtime))
  [ "$age" -le "$THRESH" ] && continue
  if echo "$PROCS" | grep -qF "$path"; then
    continue   # a process is still writing this exact log
  fi
  FOUND=1
  echo "STALLED: log=$path age=${age}s (>${THRESH}s), no result line, no process references this log path"
  echo "  inspect: ssh -o BatchMode=yes -J $JUMP_HOST $BUILDER_HOST \"podman exec $CONTAINER tail -40 $path\""
done <<< "$LOGS"

if [ "$FOUND" -eq 0 ]; then
  echo "No stalled (in-progress-looking but dead) logs found (threshold ${N_MIN}m)."
fi
