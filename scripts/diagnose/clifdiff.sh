#!/usr/bin/env bash
# clifdiff.sh -- dump and diff CLIF between two slices' checkouts, or between two already-
# captured CLIF dumps, with value/block/sig/fn numbering and Beskid's `#syntax_<file>_<node>`
# label suffix normalized out first so only a REAL structural difference shows.
#
# Usage:
#   clifdiff.sh <slice-a> <input-a.bd> <slice-b> <input-b.bd>
#       Runs `beskid_cli clif --plain <input>` with each slice's own
#       <target-dir>/<slice>/debug/beskid_cli against <workspace>/compiler-<slice>/<input>
#       (input is relative to that slice's checkout), captures stdout (the CLIF; all
#       progress/logging goes to stderr and is dropped), normalizes, diffs.
#
#   clifdiff.sh --files <fileA> <fileB>
#       Diffs two already-captured local CLIF text files (e.g. one saved before a change, one
#       after) the same normalized way. Use this for a before/after comparison within one slice.
#
# Requires: the slice's beskid_cli to already be built (this tool does not build anything).
#
# Configuration (env var, default in parens) -- same convention as crashdiag.sh:
#   BESKID_DIAG_JUMP_HOST    ssh jump host                          (root@bdziam.dev)
#   BESKID_DIAG_BUILDER_HOST ssh target host, reached via the jump   (root@10.66.0.2)
#   BESKID_DIAG_CONTAINER    podman container name on the builder    (beskid-codex-build)
#   BESKID_DIAG_WORKSPACE    builder-side checkout parent            (/workspace)
#   BESKID_DIAG_TARGET_DIR   builder-side cargo target root          (/target)
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

JUMP_HOST="${BESKID_DIAG_JUMP_HOST:-root@bdziam.dev}"
BUILDER_HOST="${BESKID_DIAG_BUILDER_HOST:-root@10.66.0.2}"
CONTAINER="${BESKID_DIAG_CONTAINER:-beskid-codex-build}"
WORKSPACE="${BESKID_DIAG_WORKSPACE:-/workspace}"
TARGET_DIR="${BESKID_DIAG_TARGET_DIR:-/target}"
SSH="ssh -o BatchMode=yes -J $JUMP_HOST $BUILDER_HOST"

if [ "${1:-}" = "--files" ]; then
  FILE_A="$2"; FILE_B="$3"
  LABEL_A="$FILE_A"; LABEL_B="$FILE_B"
  RAW_A=$(cat "$FILE_A")
  RAW_B=$(cat "$FILE_B")
elif [ $# -eq 4 ]; then
  SLICE_A="$1"; INPUT_A="$2"; SLICE_B="$3"; INPUT_B="$4"
  LABEL_A="$SLICE_A:$INPUT_A"; LABEL_B="$SLICE_B:$INPUT_B"
  RAW_A=$($SSH "podman exec $CONTAINER bash -c 'cd $WORKSPACE/compiler-$SLICE_A && $TARGET_DIR/$SLICE_A/debug/beskid_cli clif --plain $INPUT_A 2>/dev/null'")
  RAW_B=$($SSH "podman exec $CONTAINER bash -c 'cd $WORKSPACE/compiler-$SLICE_B && $TARGET_DIR/$SLICE_B/debug/beskid_cli clif --plain $INPUT_B 2>/dev/null'")
else
  echo "usage: clifdiff.sh <slice-a> <input-a.bd> <slice-b> <input-b.bd>" >&2
  echo "       clifdiff.sh --files <fileA> <fileB>" >&2
  exit 2
fi

# Drop anything before the first CLIF function header -- `beskid_cli clif` occasionally prints a
# one-line corelib-sync notice ("corelib: updated to ...") on stdout ahead of the real IR.
RAW_A=$(printf '%s\n' "$RAW_A" | sed -n '/^;; Function:/,$p')
RAW_B=$(printf '%s\n' "$RAW_B" | sed -n '/^;; Function:/,$p')

if [ -z "$RAW_A" ]; then
  echo "ERROR: no CLIF output captured for $LABEL_A (build missing? input path wrong? check stderr by running the beskid_cli command directly)" >&2
  exit 2
fi
if [ -z "$RAW_B" ]; then
  echo "ERROR: no CLIF output captured for $LABEL_B" >&2
  exit 2
fi

NORM_A=$(printf '%s' "$RAW_A" | python3 "$DIR/_clif_normalize.py")
NORM_B=$(printf '%s' "$RAW_B" | python3 "$DIR/_clif_normalize.py")

if [ "$NORM_A" = "$NORM_B" ]; then
  echo "IDENTICAL after normalization (value/block/sig/fn numbering and node-id suffixes ignored)."
  echo "  $LABEL_A"
  echo "  $LABEL_B"
  RAW_DIFF=$(diff <(printf '%s\n' "$RAW_A") <(printf '%s\n' "$RAW_B") || true)
  if [ -n "$RAW_DIFF" ]; then
    echo
    echo "(raw dumps still differ cosmetically -- normalized-away numbering/ids; not shown)"
  fi
  exit 0
fi

echo "STRUCTURAL DIFFERENCE between $LABEL_A and $LABEL_B (normalized CLIF):"
echo
diff -u <(printf '%s\n' "$NORM_A") <(printf '%s\n' "$NORM_B") --label "$LABEL_A" --label "$LABEL_B" || true
