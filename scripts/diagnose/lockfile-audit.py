#!/usr/bin/env python3
"""Read-only inventory of tracked Project.lock formats and fixture ownership."""

import argparse
import json
import os
import subprocess
from pathlib import Path


def tracked_locks(repo: Path) -> list[Path]:
    listed = subprocess.run(
        ["git", "-C", str(repo), "ls-files", "-z"],
        check=True,
        capture_output=True,
    ).stdout
    return sorted(
        (Path(os.fsdecode(raw)) for raw in listed.split(b"\0") if raw and Path(os.fsdecode(raw)).name == "Project.lock"),
        key=lambda path: path.as_posix(),
    )


def audit(repo: Path) -> dict:
    counts = {"v1": 0, "v2": 0, "other": 0}
    categories = {"source": 0, "generated": 0, "orphan": 0}
    entries = []
    for relative in tracked_locks(repo):
        path = repo / relative
        if "obj" in relative.parts:
            category = "generated"
        elif any(path.parent.glob("*.bproj")):
            category = "source"
        else:
            category = "orphan"
        try:
            if path.is_symlink():
                header = "symlink"
            else:
                with path.open("rb") as lock:
                    header = lock.readline(256).rstrip(b"\r\n").decode("ascii", errors="replace")
        except OSError:
            header = "missing-or-unreadable"
        version = {"# Project.lock v1": "v1", "# Project.lock v2": "v2"}.get(header, "other")
        counts[version] += 1
        categories[category] += 1
        entries.append({"path": relative.as_posix(), "category": category, "version": version})
    return {"repo": str(repo), "counts": counts, "categories": categories, "entries": entries}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--json", action="store_true", help="print a machine-readable inventory")
    parser.add_argument("--require-v2", action="store_true", help="exit 1 if any tracked lock is not v2")
    args = parser.parse_args()
    report = audit(args.repo.resolve())
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"Tracked Project.lock: {report['counts']}; categories: {report['categories']}")
        for entry in report["entries"]:
            print(f"{entry['version']:>5}  {entry['category']:<9}  {entry['path']}")
    return int(args.require_v2 and (report["counts"]["v1"] != 0 or report["counts"]["other"] != 0))


if __name__ == "__main__":
    raise SystemExit(main())
