#!/usr/bin/env python3
"""Run `beskid format --check` on every `.bd` file under corelib (optional corpus gate).

Run from `compiler/` after `cargo build -p beskid_cli`. Intended for nox session
`format_corpus_corelib` when `BESKID_FORMAT_CORPUS=1` is set.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

SKIP_DIR_NAMES = frozenset(
    {
        ".git",
        "target",
        "obj",
        "node_modules",
        "dist",
        ".venv",
        "__pycache__",
    }
)


def main() -> int:
    compiler_root = Path(__file__).resolve().parent.parent
    cli = compiler_root / "target" / "debug" / "beskid_cli"
    if not cli.is_file():
        cli = compiler_root / "target" / "release" / "beskid_cli"
    if not cli.is_file():
        print("error: build beskid_cli first (`cargo build -p beskid_cli`)", file=sys.stderr)
        return 1

    corelib = compiler_root / "corelib" / "beskid_corelib"
    if not corelib.is_dir():
        print(f"skip: corelib tree not present at {corelib}")
        return 0

    paths = [
        p
        for p in sorted(corelib.rglob("*.bd"))
        if not any(part in SKIP_DIR_NAMES for part in p.parts)
    ]

    failures: list[tuple[Path, str]] = []
    for path in paths:
        proc = subprocess.run(
            [str(cli), "format", "--check", str(path)],
            cwd=compiler_root,
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            failures.append((path, proc.stderr or proc.stdout or "(no output)"))

    if failures:
        for p, err in failures[:20]:
            print(f"FAIL {p.relative_to(compiler_root)}", file=sys.stderr)
            print(err, file=sys.stderr)
        if len(failures) > 20:
            print(f"... and {len(failures) - 20} more", file=sys.stderr)
        return 1

    print(f"format_corpus_check: OK ({len(paths)} .bd files checked)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
