#!/usr/bin/env python3
"""Regenerate *.expected.bd from *.input.bd using `beskid_cli format` (stdout only).

Run from the `compiler/` directory after `cargo build -p beskid_cli`.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def main() -> int:
    compiler_root = Path(__file__).resolve().parent.parent
    cli = compiler_root / "target" / "debug" / "beskid_cli"
    if not cli.is_file():
        cli = compiler_root / "target" / "release" / "beskid_cli"
    if not cli.is_file():
        print("error: build beskid_cli first (`cargo build -p beskid_cli`)", file=sys.stderr)
        return 1

    fixture_root = compiler_root / "crates" / "beskid_tests" / "fixtures" / "format"
    inputs = sorted(fixture_root.rglob("*.input.bd"))
    if not inputs:
        print("error: no *.input.bd under", fixture_root, file=sys.stderr)
        return 1

    for inp in inputs:
        out = inp.with_name(inp.name.replace(".input.bd", ".expected.bd"))
        proc = subprocess.run(
            [str(cli), "format", str(inp)],
            cwd=compiler_root,
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            print(f"error: format failed for {inp}:\n{proc.stderr}", file=sys.stderr)
            return proc.returncode
        out.write_text(proc.stdout, encoding="utf-8")
        print(f"blessed {out.relative_to(compiler_root)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
