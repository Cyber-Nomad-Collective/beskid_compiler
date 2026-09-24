#!/usr/bin/env python3
"""Summarize a Beskid corelib or runtime matrix log, from a file or stdin.

The final ``matrix:`` line is the only authoritative pass count. JIT targets can run in
parallel, so individual PASS markers are counted but never assigned to a target name.
"""

import argparse
import pathlib
import re
import sys


ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
TARGET = re.compile(r"\bRunning ([A-Za-z][A-Za-z0-9_]+)\.\.\.")
MATRIX = re.compile(r"^matrix: (\d+)/(\d+) passed, (\d+)/(\d+) failed$")
DIAGNOSTIC = re.compile(r"^\s*(?:x\s+|error:\s+)(.+)$")


def summarize(text: str) -> dict:
    clean = ANSI.sub("", text)
    result = {
        "matrix": None,
        "release_eligible": None,
        "last_target": None,
        "pass_markers": 0,
        "fail_markers": 0,
        "diagnostics": [],
    }
    seen_diagnostics = set()
    for line in clean.splitlines():
        target = TARGET.search(line)
        if target:
            result["last_target"] = target.group(1)
        matrix = MATRIX.fullmatch(line.strip())
        if matrix:
            result["matrix"] = (int(matrix.group(1)), int(matrix.group(2)))
        if line.strip() in ("release eligible: true", "release eligible: false"):
            result["release_eligible"] = line.strip().endswith("true")
        if line.startswith("PASS "):
            result["pass_markers"] += 1
        elif line.startswith("FAIL "):
            result["fail_markers"] += 1
        diagnostic = DIAGNOSTIC.match(line)
        if diagnostic and diagnostic.group(1) not in seen_diagnostics:
            seen_diagnostics.add(diagnostic.group(1))
            result["diagnostics"].append(diagnostic.group(1))
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", nargs="?", default="-", help="log path, or - for stdin")
    args = parser.parse_args()
    text = sys.stdin.read() if args.log == "-" else pathlib.Path(args.log).read_text(encoding="utf-8", errors="replace")
    result = summarize(text)
    if result["matrix"] is None:
        print("matrix: no final result yet")
    else:
        passed, total = result["matrix"]
        print(f"matrix: {passed}/{total} passed")
    if result["release_eligible"] is not None:
        print(f"release eligible: {str(result['release_eligible']).lower()}")
    print(f"last target started: {result['last_target'] or '(none)'}")
    print(f"raw PASS/FAIL markers: {result['pass_markers']}/{result['fail_markers']}")
    for diagnostic in result["diagnostics"][:8]:
        print(f"diagnostic: {diagnostic}")
    if len(result["diagnostics"]) > 8:
        print(f"... {len(result['diagnostics']) - 8} more distinct diagnostics")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
