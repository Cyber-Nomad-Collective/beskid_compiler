#!/usr/bin/env bash
# envcheck.sh <slice> -- preflight the test ENVIRONMENT on the builder for one slice, so an
# environment failure (missing corelib install, stale kit, missing binary, corrupt cache) stops
# masquerading as a product bug. Read-only except where it prints (never runs) the exact repair
# command.
#
# Checks:
#   1. The installed corelib fingerprint path ($HOME/.beskid/beskid_corelib inside the container --
#      this is the FALLBACK path shared by every slice's beskid_cli unless the binary is installed
#      under a proper <prefix>/bin layout with a bundled beskid_corelib next to it, which none of
#      these raw `cargo build` slice binaries are). Verifies it exists, has a fingerprint file, and
#      flags the concurrent-install race this shared path creates when >1 slice's beskid_cli runs
#      at once (this is a known cause behind "fingerprint installed corelib at
#      /home/<user>/.beskid/beskid_corelib: No such file or directory" -- one process's
#      remove-then-recreate racing another process's directory walk).
#   2. BESKID_RUNTIME_PREFIX kit presence/hash for the slice, by shelling out to kitcheck.sh.
#   3. The slice's built beskid_cli binary.
#   4. obj/beskid cache state under the slice's test project (present, and its age vs the source).
#
# Usage: envcheck.sh <slice> [--project <path relative to <workspace>/compiler-<slice>>]
#
# Configuration (env var, default in parens) -- same convention as crashdiag.sh/kitcheck.sh:
#   BESKID_DIAG_JUMP_HOST    ssh jump host                          (root@bdziam.dev)
#   BESKID_DIAG_BUILDER_HOST ssh target host, reached via the jump   (root@10.66.0.2)
#   BESKID_DIAG_CONTAINER    podman container name on the builder    (beskid-codex-build)
#   BESKID_DIAG_WORKSPACE    builder-side checkout parent            (/workspace)
#   BESKID_DIAG_TARGET_DIR   builder-side cargo target root          (/target)
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

JUMP_HOST="${BESKID_DIAG_JUMP_HOST:-root@bdziam.dev}"
BUILDER_HOST="${BESKID_DIAG_BUILDER_HOST:-root@10.66.0.2}"
CONTAINER="${BESKID_DIAG_CONTAINER:-beskid-codex-build}"
WORKSPACE="${BESKID_DIAG_WORKSPACE:-/workspace}"
TARGET_DIR="${BESKID_DIAG_TARGET_DIR:-/target}"
SSH="ssh -o BatchMode=yes -J $JUMP_HOST $BUILDER_HOST"

SLICE="${1:?usage: envcheck.sh <slice> [--project <path>]}"
PROJECT="corelib/beskid_corelib/tests/corelib_tests"
shift || true
while [ $# -gt 0 ]; do
  case "$1" in
    --project) PROJECT="$2"; shift 2 ;;
    *) shift ;;
  esac
done

echo "=== envcheck: slice $SLICE ==="
echo

echo "--- 1. installed corelib fingerprint path ---"
REMOTE1="
HOME_DIR=\$(podman exec $CONTAINER bash -c \"echo \\\$HOME\")
echo \"container HOME: \$HOME_DIR\"
CORELIB=\"\$HOME_DIR/.beskid/beskid_corelib\"
if podman exec $CONTAINER test -d \"\$CORELIB\"; then
  echo \"exists: \$CORELIB\"
  podman exec $CONTAINER ls -la \"\$CORELIB\" 2>&1 | head -8
  if podman exec $CONTAINER test -f \"\$CORELIB/.beskid-bundle.sha256\"; then
    echo \"fingerprint file present:\"
    podman exec $CONTAINER cat \"\$CORELIB/.beskid-bundle.sha256\"
  else
    echo \"MISSING fingerprint file .beskid-bundle.sha256 -- next beskid_cli run will treat this as\"
    echo \"corrupt/incomplete and reinstall, which is itself a race window if another slice reads\"
    echo \"mid-reinstall.\"
  fi
else
  echo \"MISSING: \$CORELIB does not exist.\"
  echo \"  repair: any beskid_cli invocation in this container recreates it automatically on next run;\"
  echo \"  if it keeps disappearing, see the concurrency note below.\"
fi
echo
echo \"concurrent beskid_cli processes right now (a shared \$HOME_DIR/.beskid/beskid_corelib path\"
echo \"means simultaneous installs from different slices can race: one deletes-and-recreates while\"
echo \"another mid-walks it, producing exactly \\\"No such file or directory\\\"):\"
podman exec $CONTAINER ps -eo pid,cmd | grep beskid_cli | grep -v grep || echo \"  (none running right now)\"
"
$SSH "$REMOTE1"

echo
echo "--- 2. runtime kit (BESKID_RUNTIME_PREFIX) ---"
if [ -x "$DIR/kitcheck.sh" ]; then
  bash "$DIR/kitcheck.sh" "$SLICE" 2>&1 || echo "(kitcheck.sh failed or no kit found for slice $SLICE -- see above)"
else
  echo "kitcheck.sh not found next to envcheck.sh -- skipping kit check."
fi

echo
echo "--- 3. built CLI ---"
CLI_CHECK=$($SSH "podman exec $CONTAINER bash -c 'test -x $TARGET_DIR/$SLICE/debug/beskid_cli && echo present || echo MISSING'")
echo "$TARGET_DIR/$SLICE/debug/beskid_cli: $CLI_CHECK"
if [ "$CLI_CHECK" = "MISSING" ]; then
  echo "  repair: ssh ... \"podman exec $CONTAINER bash -c 'cd $WORKSPACE/compiler-$SLICE && CARGO_TARGET_DIR=$TARGET_DIR/$SLICE cargo build -p beskid_cli'\""
fi

echo
echo "--- 4. obj/beskid cache state ---"
REMOTE4="
PROJDIR=$WORKSPACE/compiler-$SLICE/$PROJECT
if podman exec $CONTAINER test -d \"\$PROJDIR/obj/beskid\"; then
  echo \"present: \$PROJDIR/obj/beskid\"
  podman exec $CONTAINER find \"\$PROJDIR/obj/beskid\" -maxdepth 1
else
  echo \"no obj/beskid cache at \$PROJDIR (clean -- not itself a problem, next run repopulates it)\"
fi
"
$SSH "$REMOTE4"
echo
echo "  If a target fails with a stale-looking semantic/codegen error that doesn't match current"
echo "  source, the fail-closed repair is:"
echo "    ssh ... \"podman exec $CONTAINER rm -rf $WORKSPACE/compiler-$SLICE/$PROJECT/obj/beskid/cache\""

echo
echo "--- summary ---"
echo "Re-run the failing target only after all four checks above are clean; if the corelib"
echo "fingerprint error recurs while checks 2-4 are clean, it is very likely the cross-slice race on"
echo "the shared \$HOME/.beskid/beskid_corelib path (multiple slices' beskid_cli installing/reading"
echo "concurrently), not a product bug -- fix by giving this slice its own BESKID_CORELIB_ROOT:"
echo "    ssh ... \"podman exec $CONTAINER bash -c 'mkdir -p $WORKSPACE/.corelib-$SLICE &&"
echo "    cd $WORKSPACE/compiler-$SLICE && BESKID_CORELIB_ROOT=$WORKSPACE/.corelib-$SLICE"
echo "    CARGO_TARGET_DIR=$TARGET_DIR/$SLICE $TARGET_DIR/$SLICE/debug/beskid_cli test --plain --project"
echo "    $PROJECT --target <TARGET>'\""
