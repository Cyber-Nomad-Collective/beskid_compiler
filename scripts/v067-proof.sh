#!/usr/bin/env bash
# v067-proof.sh -- the 0.6.7 stability-proof gate. Runs, in order, before the three Woodpecker builds:
#   1. language-server soak     (beskid-soak: scripted LSP client, 30 minutes by default)
#   2. internal-error sweep     (scripts/proof/ice_sweep.py: check/build/test + mutation corpus)
#   3. onboarding check         (scripts/proof/onboarding_check.py: fresh HOME, bundle install, beskid new per template)
# All three always run; the script exits 1 when any failed, 2 on a usage or setup error.
#
# Prerequisites (built once at integration, never here):
#   cargo build -p beskid_proof --release         -> target/release/beskid-soak, beskid-mutate
#   an installed beskid CLI and beskid_lsp built with the unwinding profile (release-lsp)
#   a verified release bundle directory for step 3 (same input as scripts/install-local-toolchain-bundle.sh)
#
# Usage:
#   scripts/v067-proof.sh --beskid PATH --lsp PATH --soak-bin PATH --mutate-bin PATH \
#       [--lings DIR] [--bundle DIR --version X.Y.Z] [options]
#
#   --beskid PATH        installed beskid CLI (step 2, and the soak old-lock check)
#   --lsp PATH           beskid_lsp binary (step 1)
#   --soak-bin PATH      beskid-soak binary (step 1)
#   --mutate-bin PATH    beskid-mutate binary (step 2)
#   --compiler DIR       compiler checkout holding corelib, mods, fixtures (default: this checkout)
#   --lings DIR          beskid-lings checkout (soak corpus and sweep corpus; default none)
#   --templates-dir DIR  template packages the sweep installs before `beskid new` (beskid_templates/packages)
#   --bundle DIR         release bundle for step 3; step 3 is skipped when omitted
#   --version X.Y.Z      release version of the bundle (required with --bundle)
#   --soak-duration T    soak length: 90s, 30m, 2h (default 30m)
#   --smoke              CI mode: 60-second soak, mutation sample 10, onboarding without the editor pass
#   --seed N             seed for soak and mutation (default 1)
#   --mutation-sample N  mutants per corpus (default 60)
#   --skip LIST          comma list of soak,sweep,onboarding to skip
#   --step-timeout T     hard cap per step, in seconds (default: soak duration + 600; sweep 14400; onboarding 3600);
#                        needs `timeout` or `gtimeout` on PATH, otherwise steps run unbounded with a warning
#   --out-dir DIR        reports land here (default ./v067-proof-out)
#   --self-test          only run the unit tests of the python helpers, then exit
#
# Reports: <out-dir>/soak.txt, soak.json, sweep.txt, sweep.json, onboarding.txt, onboarding.json, summary.txt
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILER="$(cd "$HERE/.." && pwd)"

beskid="" lsp="" soak_bin="" mutate_bin="" lings="" templates_dir="" bundle="" version=""
step_timeout="" soak_duration="30m" smoke=0 seed=1 sample=60 skip="" out_dir="./v067-proof-out" self_test=0

die() { echo "v067-proof: $*" >&2; exit 2; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --beskid) beskid="${2:?}"; shift 2 ;;
    --lsp) lsp="${2:?}"; shift 2 ;;
    --soak-bin) soak_bin="${2:?}"; shift 2 ;;
    --mutate-bin) mutate_bin="${2:?}"; shift 2 ;;
    --compiler) COMPILER="${2:?}"; shift 2 ;;
    --lings) lings="${2:?}"; shift 2 ;;
    --templates-dir) templates_dir="${2:?}"; shift 2 ;;
    --bundle) bundle="${2:?}"; shift 2 ;;
    --version) version="${2:?}"; shift 2 ;;
    --soak-duration) soak_duration="${2:?}"; shift 2 ;;
    --smoke) smoke=1; shift ;;
    --seed) seed="${2:?}"; shift 2 ;;
    --mutation-sample) sample="${2:?}"; shift 2 ;;
    --skip) skip="${2:?}"; shift 2 ;;
    --step-timeout) step_timeout="${2:?}"; shift 2 ;;
    --out-dir) out_dir="${2:?}"; shift 2 ;;
    --self-test) self_test=1; shift ;;
    -h|--help) sed -n '2,/^set -uo/p' "${BASH_SOURCE[0]}" | sed '$d'; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

if [[ $self_test -eq 1 ]]; then
  exec python3 -I "$HERE/proof/test-proof-common.py"
fi

skipped() { [[ ",$skip," == *",$1,"* ]]; }

# Validate every input before anything runs: a missing binary must not surface after the soak.
need_file() { # flag path
  [[ -n "$2" ]] || die "$1 is required"
  [[ -f "$2" && -x "$2" ]] || die "$1 $2 is not an executable file"
}
if ! skipped soak; then
  [[ -n "$lsp" && -n "$soak_bin" ]] || die "soak needs --lsp and --soak-bin (or --skip soak)"
  need_file --lsp "$lsp"; need_file --soak-bin "$soak_bin"
  [[ -z "$beskid" ]] || need_file --beskid "$beskid"
  [[ -d "$COMPILER/corelib" ]] || die "no corelib under --compiler $COMPILER"
fi
if ! skipped sweep; then
  [[ -n "$beskid" && -n "$mutate_bin" ]] || die "sweep needs --beskid and --mutate-bin (or --skip sweep)"
  need_file --beskid "$beskid"; need_file --mutate-bin "$mutate_bin"
fi
if ! skipped onboarding && [[ -n "$bundle" ]]; then
  [[ -n "$version" ]] || die "--bundle needs --version"
  [[ -d "$bundle" ]] || die "--bundle $bundle is not a directory"
fi
[[ -z "$lings" || -d "$lings" ]] || die "--lings $lings is not a directory"
[[ -z "$templates_dir" || -d "$templates_dir" ]] || die "--templates-dir $templates_dir is not a directory"

timeout_cmd=""
if command -v timeout >/dev/null 2>&1; then timeout_cmd="timeout"
elif command -v gtimeout >/dev/null 2>&1; then timeout_cmd="gtimeout"
else echo "v067-proof: warning: no timeout/gtimeout on PATH; steps run without a hard cap" >&2; fi

# Seconds in a duration such as 90s, 30m, 2h.
duration_seconds() {
  case "$1" in
    *h) echo $(( ${1%h} * 3600 )) ;;
    *m) echo $(( ${1%m} * 60 )) ;;
    *s) echo "${1%s}" ;;
    *) echo "$1" ;;
  esac
}

mkdir -p "$out_dir"
out_dir="$(cd "$out_dir" && pwd)"
if [[ $smoke -eq 1 ]]; then soak_duration="60s"; sample=10; fi

st_soak="" st_sweep="" st_onboarding=""
set_status() { printf -v "st_$1" "%s" "$2"; }
get_status() { local v="st_$1"; echo "${!v:-skipped}"; }
run_step() { # name command...
  local name="$1"; shift
  if skipped "$name"; then set_status "$name" skipped; return; fi
  echo "== $name ==" >&2
  local cap="${step_timeout}"
  if [[ -z "$cap" ]]; then
    case "$name" in
      soak) cap=$(( $(duration_seconds "$soak_duration") + 600 )) ;;
      sweep) cap=14400 ;;
      *) cap=3600 ;;
    esac
  fi
  if [[ -n "$timeout_cmd" ]]; then
    "$timeout_cmd" --kill-after=60s "$cap" "$@" 2>&1 | tee "$out_dir/$name.txt"
  else
    "$@" 2>&1 | tee "$out_dir/$name.txt"
  fi
  local code=${PIPESTATUS[0]}
  if [[ $code -eq 0 ]]; then set_status "$name" pass
  elif [[ $code -eq 124 ]]; then set_status "$name" "FAIL (timed out after ${cap}s)"
  else set_status "$name" "FAIL (exit $code)"; fi
}

if ! skipped soak; then
  soak_args=(--server "$lsp" --corpus "$COMPILER/corelib" --duration "$soak_duration" --seed "$seed" --report "$out_dir/soak.json")
  [[ -z "$lings" ]] || soak_args+=(--corpus "$lings")
  [[ -z "$beskid" ]] || soak_args+=(--beskid "$beskid")
  [[ $smoke -eq 0 ]] || soak_args+=(--smoke)
  run_step soak "$soak_bin" "${soak_args[@]}"
fi

if ! skipped sweep; then
  sweep_args=(--beskid "$beskid" --mutate "$mutate_bin" --compiler "$COMPILER" --seed "$seed" --mutation-sample "$sample" --out "$out_dir/sweep.json")
  [[ -z "$lings" ]] || sweep_args+=(--lings "$lings")
  [[ -z "$templates_dir" ]] || sweep_args+=(--templates-dir "$templates_dir")
  run_step sweep python3 -I "$HERE/proof/ice_sweep.py" "${sweep_args[@]}"
fi

if ! skipped onboarding; then
  if [[ -z "$bundle" ]]; then
    set_status onboarding "skipped (no --bundle)"
  else
    onboarding_args=(--bundle "$bundle" --version "$version" --out "$out_dir/onboarding.json")
    if [[ $smoke -eq 0 && -n "$soak_bin" ]]; then onboarding_args+=(--soak "$soak_bin"); fi
    run_step onboarding python3 -I "$HERE/proof/onboarding_check.py" "${onboarding_args[@]}"
  fi
fi

{
  echo "v067-proof summary"
  for name in soak sweep onboarding; do echo "  $name: $(get_status "$name")"; done
} | tee "$out_dir/summary.txt"
for name in soak sweep onboarding; do
  [[ "$(get_status "$name")" != FAIL* ]] || exit 1
done
exit 0
