#!/usr/bin/env python3
"""List ABI-v5 contract variants left by Cargo build scripts (read-only)."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


CONTRACT = Path("crates/beskid_abi/src/generated/abi_v5_contract.rs")
EMBEDDED_JSON = re.compile(r'ABI_V5_SOURCE_JSON: &str = r#"(.*?)"#;', re.DOTALL)


def contract_facts(path: Path) -> tuple[str, int]:
    contents = path.read_bytes()
    match = EMBEDDED_JSON.search(contents.decode("utf-8"))
    if match is None:
        raise ValueError(f"{path}: no embedded ABI-v5 source JSON")
    source = json.loads(match.group(1))
    leak = next((entry for entry in source["intrinsics"] if entry["name"] == "network_report_leak"), None)
    if leak is None:
        raise ValueError(f"{path}: no network_report_leak intrinsic")
    return hashlib.sha256(contents).hexdigest(), len(leak["params"])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path)
    parser.add_argument("--target-dir", type=Path)
    args = parser.parse_args()
    repo = (args.repo or Path(__file__).resolve().parent.parent.parent).resolve()
    target_dir = (args.target_dir or repo / "target").resolve()
    canonical = repo / CONTRACT
    canonical_hash, canonical_count = contract_facts(canonical)
    print(f"tracked: {canonical_hash[:12]} network_report_leak_params={canonical_count} {canonical}")
    variants = sorted(target_dir.glob("*/build/beskid_abi-*/out/abi_v5_contract.rs"))
    if not variants:
        print(f"no ABI-v5 Cargo build outputs found under {target_dir}")
        return 0
    mismatches = 0
    for path in variants:
        digest, count = contract_facts(path)
        status = "match" if digest == canonical_hash else "different"
        mismatches += status == "different"
        print(f"{status}: {digest[:12]} network_report_leak_params={count} {path}")
    if mismatches:
        print(
            f"{mismatches} cached contract variant(s) differ. This does not identify which one a test binary linked. "
            "Stop Cargo before using `cargo clean -p beskid_abi --target-dir <target-dir>` to regenerate them."
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
