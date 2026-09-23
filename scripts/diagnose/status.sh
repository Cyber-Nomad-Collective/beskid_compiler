#!/usr/bin/env bash
# status.sh — one ssh round trip: running cargo/beskid_cli procs, load avg, newest log per
# slice with age, and last "test result:"/"Tests failed"/"EXIT=" line. For the watcher's own
# polling loop.
#
# Configuration (env var, default in parens) -- same convention as crashdiag.sh:
#   BESKID_DIAG_JUMP_HOST    ssh jump host                          (root@bdziam.dev)
#   BESKID_DIAG_BUILDER_HOST ssh target host, reached via the jump   (root@10.66.0.2)
#   BESKID_DIAG_CONTAINER    podman container name on the builder    (beskid-codex-build)
#   BESKID_DIAG_LOG_DIR      builder-side per-slice log directory    (/var/lib/beskid-codex-build/run)
set -euo pipefail

JUMP_HOST="${BESKID_DIAG_JUMP_HOST:-root@bdziam.dev}"
BUILDER_HOST="${BESKID_DIAG_BUILDER_HOST:-root@10.66.0.2}"
CONTAINER="${BESKID_DIAG_CONTAINER:-beskid-codex-build}"
LOG_DIR="${BESKID_DIAG_LOG_DIR:-/var/lib/beskid-codex-build/run}"
SSH="ssh -o BatchMode=yes -J $JUMP_HOST $BUILDER_HOST"

REMOTE="
echo \"Load avg:\"; uptime
echo
echo \"--- Running cargo/beskid_cli processes ---\"
podman exec $CONTAINER ps -eo pid,etimes,cmd 2>/dev/null | grep -E \"cargo (build|test|check)|beskid_cli\" | grep -v grep || echo \"(none)\"
echo
echo \"--- Logs (newest first) ---\"
NOW=\$(date +%s)
for f in \$(ls -t $LOG_DIR/*.log 2>/dev/null); do
  mtime=\$(stat -c \"%Y\" \"\$f\" 2>/dev/null || stat -f \"%m\" \"\$f\")
  age=\$((NOW - mtime))
  last=\$(grep -E \"test result:|Tests failed|EXIT=\" \"\$f\" | tail -1)
  printf \"%-55s age=%6ds  %s\n\" \"\$f\" \"\$age\" \"\${last:-(no result line yet)}\"
done
"

$SSH "$REMOTE"
echo
echo "(status.sh run at $(date -u +%FT%TZ))"
