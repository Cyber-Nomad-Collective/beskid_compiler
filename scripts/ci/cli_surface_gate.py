#!/usr/bin/env python3
"""Discover the release CLI surface and smoke safe paths in an isolated fixture.

Usage: python3 scripts/ci/cli_surface_gate.py /path/to/beskid_cli \
  --source-commit COMMIT --json evidence.json
Unexercised leaves are reported as such, never counted as passing tests.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


ROOT_COMMANDS = {
    "dev", "parse", "tree", "analyze", "doc", "format", "clif", "run", "test",
    "repl", "build", "mod", "import", "fetch", "lock", "update", "corelib",
    "runtime-kit", "new", "pckg", "graph", "lsp", "up", "validate-bsol", "migrate-bsol",
}
SMOKE = {
    ("parse",), ("tree",), ("analyze",), ("format",), ("graph",),
    ("new", "list"), ("up", "host-target"), ("up", "list"), ("import", "lib"), ("repl",),
    ("fetch",), ("lock",), ("update",), ("validate-bsol",),
    ("dev", "syntax", "parse"), ("dev", "syntax", "tree"),
    ("dev", "syntax", "analyze"), ("dev", "syntax", "format"),
    ("dev", "project", "graph"), ("dev", "project", "fetch"),
    ("dev", "project", "lock"), ("dev", "project", "update"),
}
SETUP_SKIP_ROOTS = {"pckg", "runtime-kit", "lsp", "mod"}
SETUP_SKIP_PATHS = {
    ("new", "install"), ("new", "uninstall"),
    ("up", "use"), ("up", "remove"), ("up", "check"),
    ("dev", "package", "registry"),
}
CONTROL_BYTES = set(range(32)) - {10, 13}


def command_names(help_text):
    names = []
    in_commands = False
    for line in help_text.splitlines():
        if line == "Commands:":
            in_commands = True
            continue
        if in_commands and line and not line[0].isspace():
            break
        if in_commands:
            match = re.match(r"  ([a-z][a-z0-9-]*)\s{2,}", line)
            if match and match.group(1) != "help":
                names.append(match.group(1))
    return names


def classify(path):
    if path in SMOKE:
        return "smoke"
    if path in SETUP_SKIP_PATHS or path[0] in SETUP_SKIP_ROOTS or path[:3] == ("dev", "package", "registry"):
        return "setup_skip"
    return "uncovered"


def run(binary, args, env, cwd, input_text=None):
    return subprocess.run(
        [str(binary), *args], cwd=cwd, env=env, input=input_text,
        capture_output=True, timeout=60, check=False,
    )


def discover(binary, env, cwd):
    rows = []
    def visit(path):
        result = run(binary, [*path, "--help"], env, cwd)
        if result.returncode != 0:
            raise RuntimeError(f"help failed for {' '.join(path)}: {result.stderr.decode(errors='replace')}")
        names = command_names(result.stdout.decode(errors="replace"))
        if path:
            rows.append({"path": " ".join(path), "kind": "branch" if names else "leaf",
                         "help_exit": result.returncode})
        for name in names:
            visit((*path, name))
        return names
    roots = set(visit(()))
    if roots != ROOT_COMMANDS:
        raise RuntimeError(f"root command drift: missing={sorted(ROOT_COMMANDS - roots)}, extra={sorted(roots - ROOT_COMMANDS)}")
    return rows


def smoke_args(path, source, project):
    tail = path[-1]
    if tail in {"parse", "tree", "format"}:
        return [str(source)], None, "Main"
    if tail == "analyze":
        return ["--plain", str(source)], None, "Analyze complete"
    if tail == "graph":
        return ["--project", str(project), "--plain", "--mermaid"], None, "flowchart"
    if path == ("new", "list"):
        return [], None, ""
    if path == ("up", "host-target"):
        return [], None, "unknown-linux"
    if path == ("up", "list"):
        return [], None, "no direct-install version is active"
    if tail in {"fetch", "lock", "update"}:
        marker = {"fetch": "Dependencies resolved", "lock": "Project.lock synchronized",
                  "update": "Workspace updated"}[tail]
        return ["--project", str(project), "--plain"], None, marker
    if path == ("validate-bsol",):
        return [str(project)], None, "ok: validated against profile"
    if path == ("import", "lib"):
        return ["libc", "--dry-run"], None, "dry-run"
    if path == ("repl",):
        return [], b":help\n:quit\n", "commands: :quit"
    raise ValueError(f"no smoke fixture for {path}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--json", type=Path, required=True)
    parser.add_argument("--source-commit", required=True, help="pinned source commit that produced the binary")
    args = parser.parse_args(argv)
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="beskid-cli-surface-") as scratch:
        root = Path(scratch)
        source = root / "Src" / "Smoke.bd"
        source.parent.mkdir()
        source.write_text("pub i64 Main() { return 0; }\n")
        project = root / "Smoke.bproj"
        project.write_text('Smoke { name = "Smoke" version = "0.1.0" }\n'
                           'target "Smoke" { kind = "App" entry = "Smoke.bd" }\n')
        env = {key: os.environ[key] for key in ("PATH", "TERM", "LANG", "BESKID_RUNTIME_PREFIX")
               if key in os.environ}
        env.update(HOME=str(root), BESKID_HOME=str(root / "beskid-home"),
                   BESKID_CORELIB_ROOT=str(root / "corelib"), OTEL_SDK_DISABLED="true")
        rows = discover(binary, env, root)
        failures = []
        for row in rows:
            if row["kind"] == "branch":
                row["status"] = "inventory_only"
                continue
            path = tuple(row["path"].split())
            row["status"] = classify(path)
            if row["status"] == "setup_skip":
                row["reason"] = "requires authenticated, destructive, remote, or installed runtime setup"
                continue
            if row["status"] == "uncovered":
                row["reason"] = "advertised leaf outside bounded safe smoke set"
                continue
            suffix, stdin, marker = smoke_args(path, source, project)
            result = run(binary, [*path, *suffix], env, root, stdin)
            output = result.stdout + result.stderr
            row["exit"] = result.returncode
            row["expected_exit"] = 0
            row["control_bytes"] = sorted(set(output) & CONTROL_BYTES)
            row["marker_seen"] = marker.encode() in output
            row["status"] = "pass" if result.returncode == 0 and not row["control_bytes"] and row["marker_seen"] else "fail"
            if row["status"] == "fail":
                row["output_tail"] = output[-1000:].decode(errors="replace")
                failures.append(row["path"])
        new_help = run(binary, ["new", "--help"], env, root)
        graph_help = run(binary, ["graph", "--help"], env, root)
        removed = run(binary, ["hi"], env, root)
        removed_picker = run(binary, ["new", "--tui"], env, root)
        contracts = {
            "hi_unknown": {"expected_exit": 2, "exit": removed.returncode,
                           "no_escape": b"\x1b" not in removed.stdout + removed.stderr},
            "new_tui_rejected": {"expected_exit": 2, "exit": removed_picker.returncode,
                                 "not_advertised": b"--tui" not in new_help.stdout,
                                 "no_escape": b"\x1b" not in removed_picker.stdout + removed_picker.stderr},
            "graph_tui_advertised": b"--tui" in graph_help.stdout,
        }
        contract_ok = (contracts["hi_unknown"]["exit"] == 2 and contracts["hi_unknown"]["no_escape"]
                       and contracts["new_tui_rejected"]["exit"] == 2
                       and contracts["new_tui_rejected"]["not_advertised"]
                       and contracts["new_tui_rejected"]["no_escape"]
                       and contracts["graph_tui_advertised"])
        if not contract_ok:
            failures.append("hi/new/graph command contract")
        counts = {status: sum(row["status"] == status for row in rows)
                  for status in ("pass", "fail", "setup_skip", "uncovered", "inventory_only")}
        evidence = {"schema": "beskid.cli-surface.v1", "binary": str(binary),
                    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                    "source_commit": args.source_commit, "counts": counts,
                    "contracts": contracts, "rows": rows}
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(evidence, indent=2) + "\n")
        print(json.dumps({"counts": counts, "failures": failures, "evidence": str(args.json)}))
        return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
