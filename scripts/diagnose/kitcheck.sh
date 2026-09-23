#!/usr/bin/env bash
# kitcheck.sh — inspect a runtime kit and explain a SourceHashMismatch.
#
# Usage:
#   kitcheck.sh <slice> [kit-dir-name]
#
#   <slice>        slice name, e.g. "rulings", "netrt" — selects
#                   <workspace>/compiler-<slice> as the source checkout to compare against.
#   [kit-dir-name] kit directory under <workspace>/verify (default: network-kit.<slice>).
#
# What it does (read-only):
#   1. Finds abi.json inside the kit (lib/beskid-runtime/abi-5/<target>/<profile>/abi.json)
#      and prints its recorded source_hash / layout_hash / target / profile / abi.json mtime.
#   2. Reads crates/beskid_abi/src/runtime_source/sources.rs LOCALLY (this worktree) to build
#      the manifest of embedded runtime/corelib .bd files (logical_path -> real repo path).
#      NOTE: src/Runtime/Mem/AbiValue.bd is hashed upstream as a generated Rust const PLUS the
#      raw file; this tool checks the raw file's mtime only and flags it as approximate.
#   3. On the remote slice checkout, reproduces the exact canonical_source_hash() algorithm
#      (len-prefixed logical path + len-prefixed content, sequential SHA-256) over every mapped
#      file except that generated prefix, and compares to abi.json's source_hash.
#   4. Lists every mapped file whose mtime is newer than abi.json's mtime — the prime suspects
#      for a SourceHashMismatch.
#
# Configuration (env var, default in parens) -- same convention as crashdiag.sh:
#   BESKID_DIAG_JUMP_HOST    ssh jump host                          (root@bdziam.dev)
#   BESKID_DIAG_BUILDER_HOST ssh target host, reached via the jump   (root@10.66.0.2)
#   BESKID_DIAG_CONTAINER    podman container name on the builder    (beskid-codex-build)
#   BESKID_DIAG_REPO_ROOT    local repo root to read sources.rs from (this script's own repo)
#   BESKID_DIAG_WORKSPACE    builder-side checkout parent            (/workspace)
set -euo pipefail

SLICE="${1:?usage: kitcheck.sh <slice> [kit-dir-name]}"
KIT_NAME="${2:-network-kit.${SLICE}}"
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DEFAULT="$(cd "$DIR/../.." && pwd)"

JUMP_HOST="${BESKID_DIAG_JUMP_HOST:-root@bdziam.dev}"
BUILDER_HOST="${BESKID_DIAG_BUILDER_HOST:-root@10.66.0.2}"
CONTAINER="${BESKID_DIAG_CONTAINER:-beskid-codex-build}"
REPO="${BESKID_DIAG_REPO_ROOT:-$REPO_DEFAULT}"
WORKSPACE="${BESKID_DIAG_WORKSPACE:-/workspace}"
SSH="ssh -o BatchMode=yes -J $JUMP_HOST $BUILDER_HOST"

SOURCES_RS="$REPO/crates/beskid_abi/src/runtime_source/sources.rs"
if [ ! -f "$SOURCES_RS" ]; then
  SOURCES_RS="$(find "$REPO" -maxdepth 9 -path '*crates/beskid_abi/src/runtime_source/sources.rs' 2>/dev/null | head -1)"
fi
if [ -z "$SOURCES_RS" ] || [ ! -f "$SOURCES_RS" ]; then
  echo "ERROR: cannot find crates/beskid_abi/src/runtime_source/sources.rs under $REPO" >&2
  exit 2
fi

# --- Step 1: build the manifest (logical_path -> repo-relative real path) locally, write to a file ---
python3 "$DIR/_kitcheck_manifest.py" "$SOURCES_RS" > "$DIR/.kitcheck_manifest.json"

# --- Step 2: copy the manifest into the container, then run the analysis script via stdin ---
REMOTE_TMP="/tmp/kitcheck_manifest_${SLICE}.json"
$SSH "cat > $REMOTE_TMP" < "$DIR/.kitcheck_manifest.json"
$SSH "podman cp $REMOTE_TMP $CONTAINER:$REMOTE_TMP"

RESULT=$($SSH "podman exec -i $CONTAINER python3 - '$SLICE' '$KIT_NAME' '$REMOTE_TMP' '$WORKSPACE'" < "$DIR/_kitcheck_remote.py")

echo "$RESULT" | python3 "$DIR/_kitcheck_report.py"
