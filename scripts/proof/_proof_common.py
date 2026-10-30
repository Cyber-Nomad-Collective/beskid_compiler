#!/usr/bin/env python3
"""Shared helpers for the 0.6.7 stability-proof scripts (ice_sweep.py, onboarding_check.py).

Standard library only. Nothing here builds Beskid; it runs an already installed `beskid`.
"""
from __future__ import annotations

import os
import re
import shutil
import signal
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

SKIP_DIRS = {"obj", ".git", "target", "node_modules", ".beskid"}

# Signatures of an internal compiler error. The whole internal diagnostic family E21xx is matched
# by one pattern (E2101..E2107 and any later member), plus E4041 and the raw texts below.
# Extra patterns come from --ice-pattern or BESKID_PROOF_ICE_PATTERNS (regexes separated by `|||`).
_BASE_PATTERNS = [
    ("E21xx internal diagnostic", r"\bE21\d\d\b"),
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


# A diagnostic's source frame quotes the user's code (`  3 │ // ... internal compiler error`), so a
# signature is matched only against the tool's own lines, never against quoted source lines.
_SOURCE_FRAME_LINE = re.compile(r"^\s*\d+\s*│")


def tool_text(output: str) -> str:
    """The output without the source lines that diagnostics quote."""
    return "\n".join(line for line in output.splitlines() if not _SOURCE_FRAME_LINE.match(line))


def classify(output: str, returncode: int | None, timed_out: bool, patterns) -> list[str]:
    """Return the failure signatures for one command result (empty list means no internal error)."""
    found = []
    output = tool_text(output)
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


MAX_OUTPUT_BYTES = 4 * 1024 * 1024


def _kill_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass


def run(command, cwd=None, timeout=600, env=None) -> RunResult:
    """Run a command in its own process group; on timeout the whole group (linker, test
    executables, grandchildren) is killed. Captured output is capped at MAX_OUTPUT_BYTES."""
    started = time.monotonic()
    try:
        process = subprocess.Popen(
            [str(c) for c in command],
            cwd=cwd,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
    except OSError as error:
        return RunResult(list(command), 127, f"cannot run {command[0]}: {error}", time.monotonic() - started, False)
    timed_out = False
    try:
        data, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        _kill_group(process)
        try:
            data, _ = process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            data = b""
    else:
        _kill_group(process)  # reap stragglers that outlived the leader
    data = data or b""
    if len(data) > MAX_OUTPUT_BYTES:
        data = data[:MAX_OUTPUT_BYTES // 2] + b"\n[output truncated]\n" + data[-MAX_OUTPUT_BYTES // 2:]
    output = data.decode("utf-8", "replace")
    return RunResult(list(command), None if timed_out else process.returncode, output, time.monotonic() - started, timed_out)


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
