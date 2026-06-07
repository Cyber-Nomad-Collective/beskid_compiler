#!/usr/bin/env python3
"""One-shot migration: Project.proj -> <name>.bproj, Workspace.proj -> <workspace>.bws."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def migrate_project(path: Path) -> None:
    text = path.read_text()
    m = re.search(r"project\s*\{", text)
    if not m:
        print(f"skip (no project block): {path}")
        return
    name_m = re.search(r'name\s*=\s*"([^"]+)"', text)
    if not name_m:
        print(f"skip (no name): {path}")
        return
    name = name_m.group(1)
    text = text.replace("project {", f"{name} {{", 1)
    text = re.sub(r'\n\s*entry\s*=\s*"Prelude\.bd"\s*\n', "\n", text)
    text = re.sub(r'\n\s*entry\s*=\s*Prelude\.bd\s*\n', "\n", text)
    out = path.with_name(f"{name}.bproj")
    out.write_text(text)
    if path != out:
        path.unlink()
    print(f"migrated -> {out}")


def migrate_workspace(path: Path) -> None:
    text = path.read_text()
    name_m = re.search(r'name\s*=\s*"([^"]+)"', text)
    if not name_m:
        print(f"skip workspace (no name): {path}")
        return
    ws_name = name_m.group(1)
    # Corelib workspace uses CoreLib.bws per plan
    if ws_name == "corelib" and "corelib" in str(path):
        out_name = "CoreLib.bws"
    else:
        out_name = f"{ws_name}.bws"
    out = path.with_name(out_name)
    out.write_text(text)
    if path != out:
        path.unlink()
    print(f"migrated workspace -> {out}")


def main() -> int:
    roots = [ROOT, ROOT.parent / "beskid_templates"]
    for base in roots:
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("Project.proj")):
            if "obj" in path.parts or "target" in path.parts:
                continue
            migrate_project(path)
        for path in sorted(base.rglob("Workspace.proj")):
            if "obj" in path.parts or "target" in path.parts:
                continue
            migrate_workspace(path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
