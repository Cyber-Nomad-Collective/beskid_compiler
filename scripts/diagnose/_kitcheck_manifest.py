#!/usr/bin/env python3
"""Internal helper for kitcheck.sh: parse crates/beskid_abi/src/runtime_source/sources.rs
and print {"exact": {logical_path: real_repo_relative_path}, "approx": {...}} as JSON."""
import re
import sys
import json

text = open(sys.argv[1]).read()

path_consts = dict(re.findall(r'pub const (CANONICAL_\w+_SOURCE_PATH): &str = "([^"]+)";', text))

source_real = {}
for m in re.finditer(
    r'const (CANONICAL_\w+_SOURCE): &str = include_str!\(concat!\(\s*env!\("CARGO_MANIFEST_DIR"\),\s*"(/[^"]+)"\s*\)\);',
    text, re.S):
    source_real[m.group(1)] = re.sub(r'^/(\.\./)+', '', m.group(2))

manifest = {}
for path_key, logical in path_consts.items():
    source_key = path_key[:-len("_PATH")]
    real = source_real.get(source_key)
    if real:
        manifest[logical] = real

inline = re.findall(
    r'logical_path: "([^"]+)"\.into\(\),\s*source: include_str!\("([^"]+)"\)\.into\(\)',
    text)
for logical, rel in inline:
    real = re.sub(r'^(\.\./)+', '', rel)
    manifest[logical] = real

manifest_approx = {}
m = re.search(r'logical_path: "(src/Runtime/Mem/AbiValue\.bd)"\.into\(\),\s*source: format!\(', text)
if m:
    manifest_approx[m.group(1)] = "runtime/beskid/src/Runtime/Mem/AbiValue.bd"
    manifest.pop(m.group(1), None)

print(json.dumps({"exact": manifest, "approx": manifest_approx}))
