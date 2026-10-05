#!/usr/bin/env bash
# Sync this worktree to the NixOS builder and run a command in the build container.
# Usage: scripts/diagnose/v053-remote.sh sync
#        scripts/diagnose/v053-remote.sh run '<command run in /workspace/compiler-v053>'
# Logs: /var/lib/beskid-codex-build/run/v053-*.log on the host (/workspace/../run is not mounted;
# use `tee /workspace/v053-<name>.log` inside commands).
set -euo pipefail
JUMP="${BESKID_DIAG_JUMP_HOST:-root@bdziam.dev}"
HOST="${BESKID_DIAG_BUILDER_HOST:-root@10.66.0.2}"
SLICE="${V053_SLICE:-compiler-v053}"
HOST_DIR="/var/lib/beskid-codex-build/work/$SLICE"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SSH=(ssh -o BatchMode=yes -o ConnectTimeout=15 -J "$JUMP" "$HOST")
case "${1:-}" in
  sync)
    rsync -az --delete --chmod=Dugo+rwx,Fugo+rw -e "ssh -o BatchMode=yes -J $JUMP" \
      --exclude target --exclude '.git' --exclude 'obj/' --exclude 'Project.lock.tmp' \
      "$ROOT/" "$HOST:$HOST_DIR/"
    echo "synced $ROOT -> $HOST:$HOST_DIR"
    ;;
  run)
    shift
    "${SSH[@]}" "podman exec -w /workspace/$SLICE -e CARGO_TARGET_DIR=/target/$SLICE beskid-codex-build bash -lc $(printf '%q' "$*")"
    ;;
  *) echo "usage: $0 sync | run '<cmd>'" >&2; exit 2 ;;
esac
