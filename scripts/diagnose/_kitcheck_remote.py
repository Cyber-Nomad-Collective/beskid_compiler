#!/usr/bin/env python3
"""Internal helper for kitcheck.sh, run INSIDE the builder container via
`podman exec <container> python3 - <slice> <kit_name> <manifest_path> <workspace>`, script on
stdin. Reproduces canonical_source_hash() over the slice checkout and reports drift vs the kit's
abi.json. `<workspace>` is the builder-side checkout parent (kitcheck.sh passes
$BESKID_DIAG_WORKSPACE, default /workspace), so this script has no hardcoded host paths."""
import json
import os
import hashlib
import sys

slice_name, kit_name, manifest_path = sys.argv[1], sys.argv[2], sys.argv[3]
workspace = sys.argv[4] if len(sys.argv) > 4 else "/workspace"
checkout = f"{workspace}/compiler-{slice_name}"
manifest = json.load(open(manifest_path))

kit_root = f"{workspace}/verify/{kit_name}"
abi_json_path = None
for root, dirs, files in os.walk(kit_root):
    if "abi.json" in files:
        abi_json_path = os.path.join(root, "abi.json")
        break
if abi_json_path is None:
    print(json.dumps({"error": f"no abi.json found under {kit_root}"}))
    sys.exit(0)

meta = json.load(open(abi_json_path))
kit_mtime = os.path.getmtime(abi_json_path)

hasher = hashlib.sha256()


def hstr(s: str):
    b = s.encode("utf-8")
    hasher.update(len(b).to_bytes(8, "little"))
    hasher.update(b)


units = []
missing = []
for logical in sorted(manifest["exact"]):
    real = manifest["exact"][logical]
    p = os.path.join(checkout, real)
    if not os.path.isfile(p):
        missing.append(real)
        continue
    src = open(p, encoding="utf-8").read()
    units.append((logical, src, os.path.getmtime(p), real))

for logical, src, mtime, real in units:
    hstr(logical)
    hstr(src)
computed = hasher.hexdigest()

newer = [(real, mtime) for (_, _, mtime, real) in units if mtime > kit_mtime]
for logical, real in manifest.get("approx", {}).items():
    p = os.path.join(checkout, real)
    if os.path.isfile(p) and os.path.getmtime(p) > kit_mtime:
        newer.append((real + " (APPROX: AbiValue.bd, generated ABI layout prefix excluded)", os.path.getmtime(p)))

print(json.dumps({
    "kit_source_hash": meta.get("source_hash"),
    "kit_layout_hash": meta.get("layout_hash"),
    "kit_target": meta.get("target", {}).get("triple"),
    "kit_profile": meta.get("profile"),
    "abi_json_path": abi_json_path,
    "kit_mtime": kit_mtime,
    "computed_hash_exact_subset": computed,
    "exact_subset_matches": computed == meta.get("source_hash"),
    "missing_files": missing,
    "files_newer_than_kit": newer,
    "workspace": workspace,
    "slice": slice_name,
}))
