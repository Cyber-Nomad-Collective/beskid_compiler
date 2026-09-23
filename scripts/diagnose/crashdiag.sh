#!/usr/bin/env bash
# crashdiag.sh -- diagnose ANY fatal-signal failure (Beskid test target, Rust test binary, or a
# plain CLI run) on any slice copy on the builder. Supersedes crashinfo.sh (kept as a thin
# wrapper -- see bottom).
#
# Usage (pick one input mode):
#   crashdiag.sh --target <name> --slice <slice> [--project <path>] [--no-rerun] [--json]
#       e.g. crashdiag.sh --target TextRegexTests --slice cov
#   crashdiag.sh --log <path-on-builder-or-local>                    [--json]
#       Parses a captured log for the "Illegal instruction"/"Segmentation fault"/... bash message
#       or an EXIT=/EXITCODE= marker.
#   crashdiag.sh --exit <code> --bin <path-on-builder>                [--json]
#       Skips log parsing; goes straight to coredump lookup for that binary.
#
# What it does, and how (evidence, not guesses):
#   1. Maps the exit/signal code using the 128+signal convention, with a note of what that signal
#      usually means IN THIS CODEBASE specifically (see SIGNAL TABLE below) -- derived from real
#      coredumps already on this builder, not assumption.
#   2. Finds the matching coredumpctl entry (newest by binary name unless a PID is known), prints
#      its backtrace, and symbolizes every frame it can via `addr2line` against the exact binary
#      (gdb/objdump have been confirmed NOT installed on at least one builder host -- reported,
#      never silently assumed absent or present; re-checked on demand, not cached here).
#   3. Splits frames into JIT-generated (unsymbolizable -- addr2line/nm find no containing symbol,
#      expected for Cranelift JIT code, which isn't a loaded ELF module) vs Rust-compiled frames.
#   4. For SIGILL, cross-references crates/beskid_isle/src/context/** trap-emission sites
#      (trap/trapz/trapnz -> bounds/null-pointer/GC-barrier/overflow) against the failing target's
#      source area to name the LIKELY trap kind, and states exactly how to confirm it for real (a
#      symbol-rich build so the trap's ISA-level operand/PC maps to a specific check).
#   5. Checks dmesg/journalctl for direct kernel corroboration ("trap invalid opcode", OOM-killer
#      lines) before finalizing the verdict -- this also catches exit 137 (SIGKILL, usually OOM)
#      being misread as a compiler bug.
#   6. Degrades honestly: every "not found"/"not installed" is stated as fact, never invented.
#
# SIGNAL TABLE (128+N convention) and what each has meant in this codebase so far, with evidence
# from real builder coredumps (re-verify per incident; this is prior-incident context, not a law):
#   132 SIGILL   Cranelift `trap`/`trapz`/`trapnz` (bounds/null/GC-barrier/overflow checks) lower
#                to an illegal instruction on x86-64. Seen via dmesg "trap invalid opcode" on a
#                faulting PID for TextRegexTests/TextRegexIntegrationTests/PestEmitGoldenTests, and
#                via files in crates/beskid_isle/src/context emitting trap instructions. Usually
#                NOT stack overflow in this codebase (see 139 below).
#   139 SIGSEGV  This codebase's stack-guard-page mechanism (runtime/beskid guard pages,
#                guarded_stack_harness) reports SIGSEGV, confirmed via real coredumps on at least
#                one builder host. Also the generic "wrote/read through a bad pointer" signal --
#                check whether the source is a guard-page test (frame count near the top of a
#                fresh stack is fine, even shallow) vs elsewhere (then suspect a bad function
#                pointer / dangling pointer instead).
#   134 SIGABRT  Rust panic / explicit abort() / assertion failure in Rust code (not JIT). Seen on
#                a builder from `isle_adapter` test binaries -- a compiler-side panic, not
#                generated-code trap.
#   137 SIGKILL  Usually the OOM killer, not the program. Always check dmesg/journalctl before
#                attributing this to product logic -- see the OOM check below.
#   133 SIGTRAP  A breakpoint/trap instruction hit outside of a debugger (e.g. `int3`) or a Rust
#                `std::intrinsics::breakpoint` -- rare; treat like SIGILL evidence-gathering but do
#                not assume it's the same Cranelift bounds-check path without checking.
#   136 SIGFPE   Integer divide-by-zero or overflow trap depending on target -- cross-check
#                beskid_isle's own overflow trap sites (see #4) since some overflow checks may
#                lower to SIGFPE-class traps on non-x86 targets even though x86-64 uses ud2 today.
#   138 SIGBUS   Misaligned access or access past a mapped file/mmap region -- check the crash site
#                for JIT-code-buffer or mmap'd runtime kit involvement before assuming JIT trap.
#
# Requires: ssh access to the builder configured as in the other scripts/diagnose/*.sh tools. No
# repo edits, no builds beyond an optional single --rerun of the failing target with
# BESKID_COMPILER_TRACE=1.
#
# Configuration (env var, default in parens):
#   BESKID_DIAG_JUMP_HOST    ssh jump host                          (root@bdziam.dev)
#   BESKID_DIAG_BUILDER_HOST ssh target host, reached via the jump   (root@10.66.0.2)
#   BESKID_DIAG_CONTAINER    podman container name on the builder    (beskid-codex-build)
#   BESKID_DIAG_REPO_ROOT    local repo root for source cross-refs   (this script's own repo)
#   BESKID_DIAG_WORKSPACE    builder-side checkout parent            (/workspace)
#   BESKID_DIAG_TARGET_DIR   builder-side cargo target root          (/target)
#   BESKID_DIAG_LOG_DIR      builder-side per-slice log directory    (/var/lib/beskid-codex-build/run)
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DEFAULT="$(cd "$DIR/../.." && pwd)"

JUMP_HOST="${BESKID_DIAG_JUMP_HOST:-root@bdziam.dev}"
BUILDER_HOST="${BESKID_DIAG_BUILDER_HOST:-root@10.66.0.2}"
CONTAINER="${BESKID_DIAG_CONTAINER:-beskid-codex-build}"
REPO="${BESKID_DIAG_REPO_ROOT:-$REPO_DEFAULT}"
WORKSPACE="${BESKID_DIAG_WORKSPACE:-/workspace}"
TARGET_DIR="${BESKID_DIAG_TARGET_DIR:-/target}"
LOG_DIR="${BESKID_DIAG_LOG_DIR:-/var/lib/beskid-codex-build/run}"
SSH="ssh -o BatchMode=yes -J $JUMP_HOST $BUILDER_HOST"

TARGET="" SLICE="" LOGARG="" EXITCODE="" BINPATH="" PROJECT="corelib/beskid_corelib/tests/corelib_tests"
JSON=0 RERUN=1
while [ $# -gt 0 ]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --slice) SLICE="$2"; shift 2 ;;
    --project) PROJECT="$2"; shift 2 ;;
    --log) LOGARG="$2"; shift 2 ;;
    --exit) EXITCODE="$2"; shift 2 ;;
    --bin) BINPATH="$2"; shift 2 ;;
    --repo) REPO="$2"; shift 2 ;;
    --jump-host) JUMP_HOST="$2"; SSH="ssh -o BatchMode=yes -J $JUMP_HOST $BUILDER_HOST"; shift 2 ;;
    --builder-host) BUILDER_HOST="$2"; SSH="ssh -o BatchMode=yes -J $JUMP_HOST $BUILDER_HOST"; shift 2 ;;
    --container) CONTAINER="$2"; shift 2 ;;
    --json) JSON=1; shift ;;
    --no-rerun) RERUN=0; shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

declare -a FINDINGS
add() { FINDINGS+=("$1"); [ "$JSON" -eq 0 ] && echo "$1"; }

signal_meaning() {
  case "$1" in
    132) echo "SIGILL|deliberate Cranelift trap/trapz/trapnz (bounds/null/GC-barrier/overflow check) lowered to an illegal instruction -- historically NOT stack overflow in this codebase" ;;
    139) echo "SIGSEGV|this codebase's stack-guard-page mechanism, OR a bad/dangling pointer dereference elsewhere" ;;
    134) echo "SIGABRT|Rust panic or explicit abort()/assertion in Rust code, not generated code" ;;
    137) echo "SIGKILL|almost always the OOM killer -- check dmesg/journalctl before blaming product logic" ;;
    133) echo "SIGTRAP|a trap/breakpoint instruction outside a debugger -- verify before assuming the same Cranelift bounds-check path as SIGILL" ;;
    136) echo "SIGFPE|integer divide-by-zero or an overflow trap on some targets" ;;
    138) echo "SIGBUS|misaligned access or access past a mapped region (check JIT code buffer / mmap'd kit)" ;;
    *) echo "unknown|not a recognized 128+signal crash code" ;;
  esac
}

# ---- resolve inputs to (SIGNAL_NUM, BINARY_NAME_OR_PATH, LOG_TAIL) ----
SIGNUM="" BIN_FOR_LOOKUP="" LOG_TAIL=""

if [ -n "$TARGET" ]; then
  [ -z "$SLICE" ] && { echo "--target requires --slice" >&2; exit 2; }
  LOG="$LOG_DIR/cov-${TARGET}.log"
  LOG_EXISTS=$($SSH "[ -f $LOG ] && echo yes || echo no")
  if [ "$LOG_EXISTS" = "yes" ]; then
    LOG_TAIL=$($SSH "grep -vE '^(DIAGTEMP|DBGLAYOUT)' $LOG | tail -15")
    RAW_MSG=$($SSH "grep -oE 'Illegal instruction|Segmentation fault|Aborted|Killed|Trace/breakpoint trap|Floating point exception|Bus error' $LOG | tail -1")
  else
    RAW_MSG=""
  fi
  BIN_FOR_LOOKUP="$TARGET_DIR/$SLICE/debug/beskid_cli"
  case "$RAW_MSG" in
    "Illegal instruction") SIGNUM=132 ;;
    "Segmentation fault") SIGNUM=139 ;;
    "Aborted") SIGNUM=134 ;;
    "Killed") SIGNUM=137 ;;
    "Trace/breakpoint trap") SIGNUM=133 ;;
    "Floating point exception") SIGNUM=136 ;;
    "Bus error") SIGNUM=138 ;;
    *) SIGNUM="" ;;
  esac
  if [ -z "$SIGNUM" ]; then
    # The target's own log often doesn't carry the shell's job-control death message (that's
    # printed by the interactive shell that ran it, not captured in a piped/redirected log) --
    # fall back to asking coredumpctl what signal the matching binary actually died with.
    CDC_SIG=$($SSH "coredumpctl list --no-legend 2>/dev/null | grep -F '$(basename "$BIN_FOR_LOOKUP")' | tail -1 | grep -oE 'SIG[A-Z]+'")
    case "$CDC_SIG" in
      SIGILL) SIGNUM=132 ;;
      SIGSEGV) SIGNUM=139 ;;
      SIGABRT) SIGNUM=134 ;;
      SIGKILL) SIGNUM=137 ;;
      SIGTRAP) SIGNUM=133 ;;
      SIGFPE) SIGNUM=136 ;;
      SIGBUS) SIGNUM=138 ;;
    esac
  fi
elif [ -n "$LOGARG" ]; then
  if [ -f "$LOGARG" ]; then
    RAW=$(cat "$LOGARG")
  else
    RAW=$($SSH "cat $LOGARG" 2>/dev/null)
  fi
  LOG_TAIL=$(printf '%s\n' "$RAW" | grep -vE '^(DIAGTEMP|DBGLAYOUT)' | tail -15)
  EXIT_MARK=$(printf '%s\n' "$RAW" | grep -oE '(EXIT(CODE)?=[0-9]+)' | tail -1 | grep -oE '[0-9]+')
  RAW_MSG=$(printf '%s\n' "$RAW" | grep -oE 'Illegal instruction|Segmentation fault|Aborted|Killed|Trace/breakpoint trap|Floating point exception|Bus error' | tail -1)
  BIN_FOR_LOOKUP=$(printf '%s\n' "$RAW" | grep -oE "${TARGET_DIR}/[A-Za-z0-9_-]+/debug/[A-Za-z0-9_]+" | tail -1)
  if [ -n "$EXIT_MARK" ]; then SIGNUM="$EXIT_MARK"; fi
  case "$RAW_MSG" in
    "Illegal instruction") SIGNUM=132 ;;
    "Segmentation fault") SIGNUM=139 ;;
    "Aborted") SIGNUM=134 ;;
    "Killed") SIGNUM=137 ;;
    "Trace/breakpoint trap") SIGNUM=133 ;;
    "Floating point exception") SIGNUM=136 ;;
    "Bus error") SIGNUM=138 ;;
  esac
elif [ -n "$EXITCODE" ]; then
  SIGNUM="$EXITCODE"
  BIN_FOR_LOOKUP="$BINPATH"
else
  echo "usage: crashdiag.sh --target <name> --slice <slice> | --log <path> | --exit <code> --bin <path>" >&2
  exit 2
fi

MEANING=$(signal_meaning "${SIGNUM:-0}")
SIGNAME=$(echo "$MEANING" | cut -d'|' -f1)
NOTE=$(echo "$MEANING" | cut -d'|' -f2)

[ "$JSON" -eq 0 ] && echo "=== crashdiag: ${TARGET:-${LOGARG:-$BINPATH}} ==="
add "exit_code=${SIGNUM:-unknown} signal=$SIGNAME"
add "codebase_note: $NOTE"

if [ -n "$LOG_TAIL" ] && [ "$JSON" -eq 0 ]; then
  echo "--- last log lines before crash ---"
  echo "$LOG_TAIL"
fi

# ---- coredumpctl lookup ----
# Many targets share the same binary name (beskid_cli), so a plain "newest dump for this binary"
# lookup can silently return a DIFFERENT target's crash. When we have a --target name, scan the
# last several matching dumps' "Command Line" for that exact --target argument and pick the
# newest one that actually matches, rather than trusting binary-name recency alone.
BIN_BASENAME=$(basename "${BIN_FOR_LOOKUP:-beskid_cli}")
if [ -n "$TARGET" ]; then
  DUMP_LINE=$($SSH "
    for pid in \$(coredumpctl list --no-legend 2>/dev/null | grep -F '$BIN_BASENAME' | awk '{print \$5}' | tail -12 | tac); do
      if coredumpctl info \"\$pid\" 2>/dev/null | grep -q -- '--target $TARGET\$\\|--target $TARGET '; then
        coredumpctl list --no-legend 2>/dev/null | awk -v p=\"\$pid\" '\$5==p'
        break
      fi
    done
  ")
else
  DUMP_LINE=$($SSH "coredumpctl list --no-legend 2>/dev/null | grep -F '$BIN_BASENAME' | tail -1")
fi
FRAME_COUNT=0 UNNAMED_FRAMES=0 NAMED_FRAMES=0 INNERMOST_SYM=""
if [ -z "$DUMP_LINE" ]; then
  add "coredump: none found for '$BIN_BASENAME' via coredumpctl on the builder host"
else
  add "coredump: $DUMP_LINE"
  PID=$(echo "$DUMP_LINE" | awk '{print $5}')
  INFO=$($SSH "coredumpctl info $PID 2>&1")
  STACK=$(echo "$INFO" | sed -n '/^[[:space:]]*Stack trace/,$p')
  FRAME_COUNT=$(echo "$STACK" | grep -cE '^[[:space:]]*#[0-9]+')
  NAMED_FRAMES=$(echo "$STACK" | grep -E '^[[:space:]]*#[0-9]+' | grep -vc 'n/a (n/a + 0x0)')
  UNNAMED_FRAMES=$((FRAME_COUNT - NAMED_FRAMES))
  add "frames: $FRAME_COUNT total, $UNNAMED_FRAMES unsymbolizable (JIT-generated, expected), $NAMED_FRAMES resolved against a loaded binary"
  if [ "$JSON" -eq 0 ]; then
    echo "--- backtrace ---"
    echo "$STACK" | head -30
  fi
  LAST_NAMED=$(echo "$STACK" | grep -E '^[[:space:]]*#[0-9]+' | grep -v 'n/a (n/a + 0x0)' | tail -1)
  OFFSET_BIN=$(echo "$LAST_NAMED" | grep -oE '\([^)]+\+ 0x[0-9a-f]+\)' | tr -d '()')
  BIN_PATH=$(echo "$OFFSET_BIN" | sed 's/ +.*//')
  OFFSET=$(echo "$OFFSET_BIN" | grep -oE '0x[0-9a-f]+$')
  if [ -n "$OFFSET" ] && [ -n "$BIN_PATH" ]; then
    ADDR2LINE=$($SSH "podman exec $CONTAINER addr2line -e $BIN_PATH -f -C $OFFSET 2>&1")
    if echo "$ADDR2LINE" | grep -q "No such file"; then
      add "symbolize: addr2line unavailable for $BIN_PATH (not present in this container, or path not shared -- stated, not guessed)"
    else
      INNERMOST_SYM=$(echo "$ADDR2LINE" | head -1)
      add "innermost_named_frame: $OFFSET -> $INNERMOST_SYM"
    fi
  fi
fi

# ---- dmesg/OOM corroboration ----
DMESG_TRAP=""
DMESG_OOM=""
if [ -n "${PID:-}" ]; then
  DMESG_TRAP=$($SSH "dmesg 2>/dev/null | grep -F \"[$PID]\" | tail -3")
fi
if [ -z "$DMESG_TRAP" ]; then
  DMESG_TRAP=$($SSH "dmesg 2>/dev/null | grep -E 'trap invalid opcode|segfault' | tail -3")
fi
if [ -n "$DMESG_TRAP" ]; then
  add "dmesg_corroboration: $(echo "$DMESG_TRAP" | tail -1)"
fi
DMESG_OOM=$($SSH "journalctl -k --no-pager 2>/dev/null | grep -iE 'killed process|out of memory' | tail -3")
if [ -n "$DMESG_OOM" ]; then
  add "OOM_EVIDENCE_PRESENT_ON_HOST (check timestamps against this crash before ruling it in/out): $(echo "$DMESG_OOM" | tail -1)"
elif [ "${SIGNUM:-0}" = "137" ]; then
  add "OOM_EVIDENCE: none found in journalctl -k, but SIGKILL with no OOM evidence is unusual -- check cgroup memory.events too"
fi

# ---- SIGILL trap-kind classification ----
if [ "${SIGNUM:-0}" = "132" ]; then
  TRAP_SITES=$(grep -rlE '\.trap(z|nz)?\(' "$REPO/crates/beskid_isle/src/context" 2>/dev/null)
  add "trap_emission_sites: $(echo "$TRAP_SITES" | xargs -n1 basename | tr '\n' ',' | sed 's/,$//')"
  if [ -n "$TARGET" ]; then
    LIKELY=""
    grep -qi "regex\|pest\|parser" <<< "$TARGET" && LIKELY="bounds/array-index checks in recursive-descent parsing (Regex.bd/Pest -- Array/Collections access is the most trap-dense path there)"
    [ -n "$LIKELY" ] && add "likely_trap_kind: $LIKELY"
  fi
  add "confirm_how: rebuild beskid_cli with debug symbols retained (already the case for a plain 'cargo build', so this should already work) plus gdb on a machine that has it, OR add a symbol-rich JIT frame map -- confirm gdb/objdump availability on this builder before relying on them, this tool does not assume either way."
fi

# ---- verdict ----
if [ "${SIGNUM:-0}" = "132" ]; then
  VERDICT="SIGILL: evidence points at a deliberate Cranelift trap in JIT-generated code (bounds/null/GC-barrier/overflow check), not stack overflow or memory corruption."
elif [ "${SIGNUM:-0}" = "139" ]; then
  VERDICT="SIGSEGV: consistent with this codebase's stack-guard-page mechanism if the crash site is guard-related; otherwise treat as a bad/dangling pointer. Frame count alone does not decide this -- a guard-page hit can be a shallow trace."
elif [ "${SIGNUM:-0}" = "137" ]; then
  VERDICT="SIGKILL: check the OOM evidence line above before treating this as a product bug."
else
  VERDICT="$SIGNAME: $NOTE"
fi
add "verdict: $VERDICT"

if [ "$RERUN" -eq 1 ] && [ -n "$TARGET" ] && [ -n "$SLICE" ] && [ "$JSON" -eq 0 ]; then
  echo
  echo "--- re-run with BESKID_COMPILER_TRACE=1 (best-effort) ---"
  $SSH "podman exec $CONTAINER bash -c 'cd $WORKSPACE/compiler-$SLICE && BESKID_COMPILER_TRACE=1 BESKID_RUNTIME_PREFIX=$WORKSPACE/verify/network-kit.$SLICE timeout 120 $TARGET_DIR/$SLICE/debug/beskid_cli test --plain --project $PROJECT --target $TARGET 2>&1 | tail -25; echo RERUN_EXIT=\$?'"
fi

if [ "$JSON" -eq 1 ]; then
  python3 - "${FINDINGS[@]}" <<'PYEOF'
import json, sys
lines = sys.argv[1:]
print(json.dumps({"findings": lines}, indent=2))
PYEOF
fi
