#!/usr/bin/env python3
"""Internal helper for kitcheck.sh: format the JSON result from _kitcheck_remote.py (stdin)."""
import json
import sys
import time

try:
    d = json.load(sys.stdin)
except Exception as e:
    print("could not parse remote result:", e)
    sys.exit(2)

if "error" in d:
    print("ERROR:", d["error"])
    sys.exit(2)

print(f"Kit: {d['abi_json_path']}")
print(f"  target={d['kit_target']} profile={d['kit_profile']}")
print(f"  source_hash={d['kit_source_hash']}")
print(f"  layout_hash={d['kit_layout_hash']}")
print(f"  abi.json mtime={time.strftime('%Y-%m-%d %H:%M:%S', time.gmtime(d['kit_mtime']))} UTC")
print()
note = "NOTE: hash covers all mapped files exactly except AbiValue.bd's generated ABI-layout prefix (checked separately by mtime only)."
if d["exact_subset_matches"]:
    print("Full-corpus hash: EXACT MATCH on the non-AbiValue subset.")
else:
    print("Full-corpus hash: MISMATCH on the non-AbiValue subset (source_hash also covers AbiValue.bd, so a mismatch here does not by itself localize the changed file).")
print(note)

if d["missing_files"]:
    print("\nMISSING on remote checkout (manifest drift -- sources.rs and slice tree disagree):")
    for f in d["missing_files"]:
        print(f"  {f}")

newer = d.get("files_newer_than_kit", [])
workspace = d.get("workspace", "/workspace")
slice_name = d.get("slice", "<slice>")
if newer:
    print(f"\nFiles modified AFTER the kit was built ({len(newer)}) -- prime suspects for the mismatch:")
    for f, mtime in sorted(newer, key=lambda x: -x[1]):
        print(f"  {time.strftime('%Y-%m-%d %H:%M:%S', time.gmtime(mtime))} UTC  {f}")
    print(f"\nRebuild: <target-dir>/{slice_name}/debug/beskid_cli runtime-kit build-native-host --prefix {workspace}/verify/network-kit.{slice_name} --profile debug")
else:
    print("\nNo mapped source file is newer than the kit. If SourceHashMismatch still fires, the kit"
          " was built from a different worktree/commit, or the beskid_cli binary itself embeds a"
          " different corpus (rebuild beskid_cli first, then the kit).")
