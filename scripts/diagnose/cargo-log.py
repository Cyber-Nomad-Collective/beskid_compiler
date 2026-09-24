#!/usr/bin/env python3
"""Summarize Cargo test binaries and failures from a file or stdin."""

import argparse
import pathlib
import re
import sys


ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
RUNNING = re.compile(r"^\s+Running (?:tests[/\\]|unittests .+[/\\])([^/\\ ]+)\b")
RESULT = re.compile(r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;")


def summarize(log: str) -> dict:
    """Return completed binaries, the active binary, and failed test names."""
    complete = []
    failures = []
    active = None
    for line in ANSI.sub("", log).splitlines():
        running = RUNNING.search(line)
        if running:
            active = running.group(1)
            continue
        result = RESULT.match(line)
        if result and active:
            complete.append((active, int(result.group(1)), int(result.group(2))))
            active = None
        if line.startswith("test ") and line.endswith(" ... FAILED"):
            failures.append(line)
    return {"complete": complete, "active": active, "failures": failures}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", nargs="?", default="-", help="log path, or - for stdin")
    args = parser.parse_args()
    log = sys.stdin.read() if args.log == "-" else pathlib.Path(args.log).read_text(encoding="utf-8", errors="replace")
    result = summarize(log)
    passed = sum(item[1] for item in result["complete"])
    failed = sum(item[2] for item in result["complete"])
    print(f"completed binaries: {len(result['complete'])}; tests: {passed} passed, {failed} failed")
    print(f"active binary: {result['active'] or '(none)'}")
    for failure in result["failures"]:
        print(f"failure: {failure}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
