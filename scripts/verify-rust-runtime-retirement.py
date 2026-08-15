#!/usr/bin/env python3
"""Fail closed when a retired Rust runtime surface reappears."""

from __future__ import annotations

import re
import sys
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[1]
RETIRED = {
    "beskid_" + "runtime",
    "beskid_" + "runtime_" + "handlers",
    "beskid_" + "host",
    "beskid_" + "differential_" + "tests",
}
FORBIDDEN_SOURCE = {
    "interop_" + "dispatch",
    "Dispatch" + "Route",
    "Dispatch" + "ReturnGroup",
    "dispatch_" + "envelope",
    "dispatch_" + "tags",
    "dispatch_" + "lookup",
    "rust-runtime-" + "differential",
    "C" + "-unwind",
}


def fail(message: str) -> None:
    print(f"runtime retirement verification failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def verify_workspace() -> None:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    members = set(manifest["workspace"]["members"])
    for package in RETIRED:
        relative = f"crates/{package}"
        if relative in members or (ROOT / relative).exists():
            fail(f"retired package remains: {relative}")


def verify_manifests() -> None:
    for manifest_path in ROOT.rglob("Cargo.toml"):
        if any(part in {"target", "obj", "vendor"} for part in manifest_path.parts):
            continue
        text = manifest_path.read_text()
        for package in RETIRED:
            if re.search(rf"(?m)^\s*{re.escape(package)}\s*=", text):
                fail(f"retired dependency in {manifest_path.relative_to(ROOT)}")
        if "rust-runtime-" + "differential" in text:
            fail(f"retired feature in {manifest_path.relative_to(ROOT)}")


def verify_sources() -> None:
    roots = [ROOT / "crates", ROOT / "scripts"]
    for root in roots:
        for source in root.rglob("*"):
            if not source.is_file() or source.suffix not in {
                ".rs",
                ".py",
                ".sh",
                ".toml",
            }:
                continue
            if source.resolve() == Path(__file__).resolve() or any(
                part in {"target", "obj"} for part in source.parts
            ):
                continue
            text = source.read_text(errors="replace")
            for token in FORBIDDEN_SOURCE:
                if token in text:
                    fail(f"retired token in {source.relative_to(ROOT)}")


def main() -> None:
    verify_workspace()
    verify_manifests()
    verify_sources()
    print("Rust runtime retirement verification passed")


if __name__ == "__main__":
    main()
