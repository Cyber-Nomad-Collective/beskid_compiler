#!/usr/bin/env python3
"""Verify shutdown fails closed and reports the actual pending network operation."""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path


DIAGNOSTIC = re.compile(
    r"^beskid network leak backend=(?:epoll|kqueue|iocp) "
    r"slot=(\d+) generation=(\d+) owner=(\d+) resource_kind=1 "
    r"operation=accept winner=cancelled leak_count=1$",
    re.MULTILINE,
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path, help="path to the built beskid_cli")
    parser.add_argument("--project", required=True, type=Path, help="pending network accept fixture project")
    parser.add_argument("--timeout", type=int, default=600, help="maximum seconds for compile and run")
    args = parser.parse_args()

    env = os.environ.copy()
    missing = [name for name in ("BESKID_RUNTIME_PREFIX", "BESKID_CORELIB_ROOT") if not env.get(name)]
    if missing:
        parser.error(f"required environment variables are missing: {', '.join(missing)}")

    command = [str(args.cli), "run", "--plain", "--project", str(args.project)]
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=args.timeout, env=env, check=False)
    except subprocess.TimeoutExpired as error:
        print(f"fixture exceeded {args.timeout}s", file=sys.stderr)
        if error.stdout:
            print(error.stdout, file=sys.stderr)
        if error.stderr:
            print(error.stderr, file=sys.stderr)
        return 1

    output = result.stdout + result.stderr
    diagnostics = list(DIAGNOSTIC.finditer(output))
    if result.returncode != 101 or len(diagnostics) != 1:
        print(
            f"expected one pending-accept shutdown diagnostic and exit 101; "
            f"got exit {result.returncode} and {len(diagnostics)} matching lines",
            file=sys.stderr,
        )
        print(output, file=sys.stderr)
        return 1
    diagnostic = diagnostics[0].group(0)
    if "descriptor=" in diagnostic or re.search(r"\bfd=\d+", diagnostic):
        print("network diagnostic exposed a native descriptor", file=sys.stderr)
        print(output, file=sys.stderr)
        return 1

    slot, generation, owner = diagnostics[0].groups()
    print(
        f"PASS: runtime shutdown exited {result.returncode} after one pending accept leak "
        f"(slot={slot}, generation={generation}, owner={owner}, winner=cancelled, leak_count=1)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
