#!/usr/bin/env python3
"""Shared helpers for the 0.6.7 stability-proof scripts (ice_sweep.py, onboarding_check.py).

Standard library only. Nothing here builds Beskid; it runs an already installed `beskid`.
"""
from __future__ import annotations

import os
import re
import shutil
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

SKIP_DIRS = {"obj", ".git", "target", "node_modules", ".beskid"}

# Signatures of an internal compiler error. Until every internal failure is routed to one coded
# diagnostic, the codes E2101/E2102/E4041 and the raw texts
# below are the evidence. Extra patterns (for the floor's code) come from --ice-pattern or
# BESKID_PROOF_ICE_PATTERNS (regexes separated by `|||`).
_BASE_PATTERNS = [
    ("E2101", r"\bE2101\b"),
    ("E2102", r"\bE2102\b"),
    ("E4041", r"\bE4041\b"),
    ("internal compiler error", r"internal compiler error"),
    ("MissingRuleOrFact", r"MissingRuleOrFact"),
    ("unavailable until port", r"is unavailable until"),
    ("Invalid build request", r"Invalid build request"),
    ("ISLE emission failed", r"ISLE emission failed"),
    ("Verification(", r"Verification\("),
    ("rust panic", r"panicked at|thread '[^']*' panicked"),
    ("rust backtrace hint", r"RUST_BACKTRACE"),
    ("stack overflow", r"has overflowed its stack|stack overflow"),
    ("isle invariant trap", r"unreachable_or_isle_invariant"),
]


def ice_patterns(extra=()):
    patterns = list(_BASE_PATTERNS)
    env_extra = os.environ.get("BESKID_PROOF_ICE_PATTERNS", "")
    for raw in [*extra, *[p for p in env_extra.split("|||") if p]]:
        patterns.append((f"custom:{raw}", raw))
    return [(name, re.compile(regex)) for name, regex in patterns]


def classify(output: str, returncode: int | None, timed_out: bool, patterns) -> list[str]:
    """Return the failure signatures for one command result (empty list means no internal error)."""
    found = []
    if timed_out:
        found.append("timeout")
    if returncode is not None and returncode < 0:
        found.append(f"killed by signal {-returncode}")
    for name, regex in patterns:
        if regex.search(output):
            found.append(name)
    return found


@dataclass
class RunResult:
    command: list
    returncode: int | None
    output: str
    seconds: float
    timed_out: bool


def run(command, cwd=None, timeout=600, env=None) -> RunResult:
    started = time.monotonic()
    try:
        completed = subprocess.run(
            [str(c) for c in command],
            cwd=cwd,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
            text=True,
            errors="replace",
        )
        return RunResult(list(command), completed.returncode, completed.stdout or "", time.monotonic() - started, False)
    except subprocess.TimeoutExpired as expired:
        out = expired.stdout.decode("utf-8", "replace") if isinstance(expired.stdout, bytes) else (expired.stdout or "")
        return RunResult(list(command), None, out, time.monotonic() - started, True)
    except OSError as error:
        return RunResult(list(command), 127, f"cannot run {command[0]}: {error}", time.monotonic() - started, False)


def excerpt(output: str, patterns, width: int = 400) -> str:
    """The lines around the first signature match, trimmed."""
    lines = output.splitlines()
    for index, line in enumerate(lines):
        if any(regex.search(line) for _, regex in patterns):
            chunk = "\n".join(lines[max(0, index - 1): index + 3])
            return chunk[:width]
    return "\n".join(lines[-4:])[:width]


def discover_projects(root: Path):
    """Directories under root holding a Beskid manifest (*.bproj), skipping build output and templates."""
    found = []
    for directory, subdirs, files in os.walk(root):
        subdirs[:] = sorted(d for d in subdirs if d not in SKIP_DIRS)
        manifests = [f for f in files if f.endswith(".bproj") and "{{" not in f]
        if manifests:
            found.append(Path(directory))
    return found


def owning_project(path: Path, root: Path):
    """Nearest ancestor of `path` (up to root) that holds a *.bproj."""
    current = path.parent
    while True:
        if any(f.endswith(".bproj") and "{{" not in f for f in os.listdir(current)):
            return current
        if current == root or current.parent == current:
            return None
        current = current.parent


def copy_tree(source: Path, target: Path):
    def ignore(_directory, names):
        return [n for n in names if n in SKIP_DIRS]

    shutil.copytree(source, target, ignore=ignore, symlinks=True)


_TEMPLATE_LINE = re.compile(r"^\s+(\S+)\s+—\s+.*\((Project|Workspace|Item)\)")


def parse_template_list(text: str):
    """Project and workspace templates from `beskid new --list` output as (short_name, kind)."""
    found = []
    for line in text.splitlines():
        match = _TEMPLATE_LINE.match(line)
        if match and match.group(2) != "Item" and match.group(1) not in [f[0] for f in found]:
            found.append((match.group(1), match.group(2)))
    return found


def discover_templates(beskid: str, env=None):
    result = run([beskid, "new", "--list"], timeout=120, env=env)
    return parse_template_list(result.output), result
