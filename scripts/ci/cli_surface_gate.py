#!/usr/bin/env python3
"""Discover the release CLI surface and smoke safe paths in an isolated fixture.

Usage: python3 scripts/ci/cli_surface_gate.py /path/to/beskid_cli \
  --expected-sha256 SHA256 --json evidence.json
Unexercised leaves are reported as such, never counted as passing tests.
"""

import argparse
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import threading


ROOT_COMMANDS = {
    "dev", "parse", "tree", "analyze", "doc", "format", "clif", "run", "test",
    "repl", "build", "mod", "import", "fetch", "lock", "update", "corelib",
    "runtime-kit", "new", "pckg", "graph", "lsp", "up", "validate-bsol", "migrate-bsol",
}
SMOKE = {
    ("parse",), ("tree",), ("analyze",), ("format",), ("graph",),
    ("new", "list"), ("up", "host-target"), ("up", "list"), ("up", "check"), ("import", "lib"), ("repl",),
    ("fetch",), ("lock",), ("update",), ("validate-bsol",),
    ("dev", "syntax", "parse"), ("dev", "syntax", "tree"),
    ("dev", "syntax", "analyze"), ("dev", "syntax", "format"),
    ("dev", "project", "graph"), ("dev", "project", "fetch"),
    ("dev", "project", "lock"), ("dev", "project", "update"),
    ("doc",), ("clif",), ("run",), ("test",), ("build",), ("corelib",), ("migrate-bsol",),
    ("dev", "syntax", "doc"), ("dev", "syntax", "clif"),
    ("dev", "build", "compile"), ("dev", "build", "test"), ("dev", "build", "corelib"),
    ("pckg", "pack"), ("dev", "package", "registry", "pack"),
    *(("pckg", name) for name in ("list", "search", "details", "versions", "download", "whoami")),
    *(("dev", "package", "registry", name) for name in ("list", "search", "details", "versions", "download", "whoami")),
}
SETUP_SKIPS = {
    ("mod", "rebuild"): "requires a compiler Mod project and matching runtime kit fixture",
    ("mod", "clean"): "removes compiler Mod cache; isolated populated cache fixture required",
    ("runtime-kit", "build"): "requires verified prebuilt runtime libraries and empty installation prefix",
    ("runtime-kit", "build-native-host"): "native runtime build requires isolated build capacity",
    ("runtime-kit", "build-matrix"): "requires verified debug/release runtime libraries",
    ("new", "install"): "mutates template cache; package fixture required",
    ("new", "uninstall"): "removes cached template; populated isolated cache fixture required",
    ("lsp", "install"): "downloads and replaces managed language server",
    ("up", "use"): "changes active direct-install version; populated isolated store required",
    ("up", "remove"): "removes installed direct-download version; populated isolated store required",
    ("pckg", "upload"): "publishes artifact; authenticated local registry fixture required",
    ("pckg", "configure"): "persists API key; credential fixture excluded",
    ("pckg", "yank"): "changes registry publication state; authenticated local registry fixture required",
    ("pckg", "unyank"): "changes registry publication state; authenticated local registry fixture required",
}
CONTROL_BYTES = set(range(32)) - {10}


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
    if path in SETUP_SKIPS or (path[:3] == ("dev", "package", "registry")
                               and ("pckg", *path[3:]) in SETUP_SKIPS):
        return "setup_skip"
    return "uncovered"


def release_failures(rows):
    return [f'{row["path"]}:{row["status"]}' for row in rows if row["status"] in {"fail", "uncovered"}]


def source_provenance():
    return {"status": "unverified", "commit": None, "external_receipt_required": True}


def output_evidence(stdout, stderr):
    return {
        "stdout_base64": base64.b64encode(stdout).decode("ascii"),
        "stderr_base64": base64.b64encode(stderr).decode("ascii"),
        "stdout_text": stdout.decode("utf-8", errors="backslashreplace"),
        "stderr_text": stderr.decode("utf-8", errors="backslashreplace"),
        "control_bytes": sorted(set(stdout + stderr) & CONTROL_BYTES),
    }


def run_pty(binary, args, env, cwd):
    import errno
    import fcntl
    import pty
    import select
    import signal
    import struct
    import termios
    import time

    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))

    def attach_terminal():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

    process = subprocess.Popen([str(binary), *args], cwd=cwd, env=env, stdin=slave,
                               stdout=slave, stderr=slave, preexec_fn=attach_terminal)
    os.close(slave)
    transcript = bytearray()
    sent_quit = False
    deadline = time.monotonic() + 30
    timed_out = False
    try:
        while process.poll() is None:
            if time.monotonic() >= deadline:
                timed_out = True
                os.killpg(process.pid, signal.SIGKILL)
                break
            if select.select([master], [], [], 0.05)[0]:
                try:
                    chunk = os.read(master, 65536)
                except OSError as error:
                    if error.errno == errno.EIO:
                        break
                    raise
                transcript.extend(chunk)
                if len(transcript) > 2_000_000:
                    timed_out = True
                    os.killpg(process.pid, signal.SIGKILL)
                    break
                if b"\x1b[?1049h" in transcript and not sent_quit:
                    os.write(master, b"q\n")
                    sent_quit = True
        process.wait()
        while select.select([master], [], [], 0.05)[0]:
            try:
                chunk = os.read(master, 65536)
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
            if not chunk:
                break
            transcript.extend(chunk)
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        os.close(master)
    return {"exit": process.returncode, "timed_out": timed_out, "quit_sent": sent_quit,
            "transcript_base64": base64.b64encode(transcript).decode("ascii"),
            "transcript_text": transcript.decode("utf-8", errors="backslashreplace")}


def graph_tui_rendered(transcript):
    return ("┌".encode() in transcript and "│Smoke│".encode() in transcript
            and b"flowchart" not in transcript)


def ordinary_pty_clean(transcript):
    return b"\x1b" not in transcript and not (set(transcript) & (set(range(32)) - {10, 13}))


def start_registry_mock():
    package = {
        "id": "cli-gate-id", "name": "beskid.tools.cli-gate", "description": "CLI gate fixture",
        "category": "Tool", "packageKind": "tool", "template": None,
        "repositoryUrl": None, "websiteUrl": None, "tags": [], "isPublic": True,
        "totalDownloads": 0, "updatedAtUtc": "2026-10-01T00:00:00Z",
        "pendingReviewsCount": 0, "averageRating": 0.0,
    }
    health = {"state": "healthy", "subState": "stable", "score": 1.0}
    for group in ("updateRate", "downloads", "reviews"):
        health.update({group + "State": "healthy", group + "SubState": "stable",
                       group + "Normalized": 1.0, group + "Weight": 1.0})
    details = {"package": package, "versions": [], "dependencies": [],
               "dependentsCount": 0, "readme": None, "health": health}
    routes = {
        "/api/packages": ("application/json", b"[]"),
        "/api/search?q=cli": ("application/json", b"[]"),
        "/api/packages/beskid.tools.cli-gate": ("application/json", json.dumps(details).encode()),
        "/api/packages/beskid.tools.cli-gate/versions": ("application/json", b"[]"),
        "/api/packages/beskid.tools.cli-gate/versions/0.1.0/download": ("application/octet-stream", b"fixture-package"),
        "/api/users/me": ("application/json", b'{"isAuthenticated":false,"userId":null,"email":null,"isPublisher":false}'),
    }

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            response = routes.get(self.path)
            if response is None:
                self.send_error(404)
                return
            content_type, body = response
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_port}"


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


def smoke_args(path, source, project, test_project, migration_project, root, registry_url):
    tail = path[-1]
    case_root = root / "cases" / "-".join(path)
    if tail in {"parse", "tree", "format"}:
        return [str(source)], None, "Main"
    if tail == "analyze":
        return ["--plain", str(source)], None, "Analyze complete"
    if tail == "graph":
        return ["--project", str(project), "--plain", "--mermaid"], None, "flowchart"
    if path == ("new", "list"):
        return [], None, ""
    if path == ("up", "host-target"):
        return [], None, ""
    if path == ("up", "list"):
        return [], None, "no direct-install version is active"
    if path == ("up", "check"):
        return [], None, f"release manifest: {registry_url}/release.json"
    if tail in {"fetch", "lock", "update"}:
        marker = {"fetch": "Dependencies resolved", "lock": "Project.lock synchronized",
                  "update": "Workspace updated"}[tail]
        return ["--project", str(project), "--plain"], None, marker
    if path == ("validate-bsol",):
        return [str(project)], None, "ok: validated against profile"
    if tail == "doc":
        return ["--project", str(project), "--out", str(case_root / "api-doc")], None, ""
    if tail == "clif":
        return ["--project", str(project), "--plain"], None, "CLIF ready"
    if tail in {"build", "compile"}:
        return ["--project", str(project), "--kind", "object", "--output", str(case_root / "Smoke.o"), "--plain"], None, "Build complete"
    if tail == "run":
        return ["--project", str(project), "--plain"], None, "Run complete"
    if tail == "test":
        return ["--project", str(test_project), "--plain"], None, "Result: passed=1, failed=0"
    if tail == "corelib":
        return ["--output", str(case_root / "corelib-copy")], None, "Generated Beskid corelib project"
    if path == ("migrate-bsol",):
        return ["--to", "project.v2", str(migration_project)], None, "Migration"
    if tail == "pack":
        return ["--package", "beskid.tools.cli-gate", "--source",
                str(root / "package-source"), "--output", str(case_root / "package.bpk"),
                "--package-kind", "tool", "--skip-docs"], None, "Packed artifact"
    if tail == "list":
        return [], None, "No packages found."
    if tail == "search":
        return ["cli"], None, "No packages matched"
    if tail == "details":
        return ["beskid.tools.cli-gate"], None, "downloads=0 dependents=0"
    if tail == "versions":
        return ["beskid.tools.cli-gate"], None, "No versions found"
    if tail == "download":
        return ["beskid.tools.cli-gate", "--version", "0.1.0", "--output", str(case_root / "download.bpk")], None, "Downloaded"
    if tail == "whoami":
        return [], None, "authenticated=false"
    if path == ("import", "lib"):
        return ["libc", "--dry-run"], None, "dry-run"
    if path == ("repl",):
        return [], b":help\n:quit\n", "commands: :quit"
    raise ValueError(f"no smoke fixture for {path}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--json", type=Path, required=True)
    parser.add_argument("--expected-sha256", required=True, help="checksum independently recorded when binary was built")
    args = parser.parse_args(argv)
    binary = args.binary.resolve(strict=True)
    binary_sha256 = hashlib.sha256(binary.read_bytes()).hexdigest()
    if binary_sha256 != args.expected_sha256.lower():
        raise RuntimeError(f"binary checksum mismatch: expected {args.expected_sha256}, got {binary_sha256}")
    with tempfile.TemporaryDirectory(prefix="beskid-cli-surface-") as scratch:
        root = Path(scratch)
        source = root / "Src" / "Smoke.bd"
        source.parent.mkdir()
        source.write_text("pub i64 Main() { return 0; }\ntest Smoke { }\n")
        project = root / "Smoke.bproj"
        project.write_text('Smoke { name = "Smoke" version = "0.1.0" }\n'
                           'target "Smoke" { kind = "App" entry = "Smoke.bd" }\n')
        test_root = root / "test-project"
        (test_root / "Src").mkdir(parents=True)
        (test_root / "Src" / "Smoke.bd").write_text(source.read_text())
        test_project = test_root / "Test.bproj"
        test_project.write_text('Test { name = "Test" version = "0.1.0" }\n'
                                'target "Test" { kind = "Test" entry = "Smoke.bd" }\n')
        migration_project = root / "Migration.bproj"
        migration_project.write_text('Migration { name = "Migration" version = "0.1.0" }\n')
        (root / "package-source").mkdir()
        (root / "package-source" / "README.md").write_text("CLI gate tool package\n")
        env = {key: os.environ[key] for key in ("PATH", "TERM", "LANG", "BESKID_RUNTIME_PREFIX")
               if key in os.environ}
        env.update(HOME=str(root), BESKID_HOME=str(root / "beskid-home"),
                   BESKID_CORELIB_ROOT=str(root / "corelib"), OTEL_SDK_DISABLED="true")
        registry_server, registry_url = start_registry_mock()
        env["BESKID_RELEASE_MANIFEST_URL"] = f"{registry_url}/release.json"
        rows = discover(binary, env, root)
        failures = []
        for row in rows:
            if row["kind"] == "branch":
                row["status"] = "inventory_only"
                continue
            path = tuple(row["path"].split())
            row["status"] = classify(path)
            if row["status"] == "setup_skip":
                key = ("pckg", *path[3:]) if path[:3] == ("dev", "package", "registry") else path
                row["reason"] = SETUP_SKIPS[key]
                continue
            if row["status"] == "uncovered":
                row["reason"] = "advertised leaf outside bounded safe smoke set"
                continue
            case_root = root / "cases" / "-".join(path)
            case_root.mkdir(parents=True)
            suffix, stdin, marker = smoke_args(path, source, project, test_project, migration_project, root, registry_url)
            invocation = [*path, *suffix]
            if path[0] == "pckg" and path[1] != "pack":
                invocation = ["pckg", "--base-url", registry_url, *path[1:], *suffix]
            elif path[:3] == ("dev", "package", "registry") and path[3] != "pack":
                invocation = ["dev", "package", "registry", "--base-url", registry_url, *path[3:], *suffix]
            result = run(binary, invocation, env, root, stdin)
            output = result.stdout + result.stderr
            row["exit"] = result.returncode
            row["expected_exit"] = 0
            row.update(output_evidence(result.stdout, result.stderr))
            row["argv"] = invocation
            row["marker_seen"] = marker.encode() in output
            if path == ("up", "host-target"):
                row["marker_seen"] = bool(re.fullmatch(rb"[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+){2,}\n?", result.stdout))
            if path == ("up", "check"):
                row["marker_seen"] = result.stdout.decode("utf-8", errors="replace").strip() == marker
            if tail := path[-1]:
                if tail in {"build", "compile"}:
                    row["marker_seen"] = row["marker_seen"] and (case_root / "Smoke.o").is_file()
                elif tail == "doc":
                    row["marker_seen"] = row["marker_seen"] and (case_root / "api-doc" / "api.json").is_file()
                elif tail == "corelib":
                    row["marker_seen"] = row["marker_seen"] and any((case_root / "corelib-copy").glob("*.bws"))
                elif tail == "pack":
                    row["marker_seen"] = row["marker_seen"] and (case_root / "package.bpk").is_file()
                elif tail == "download":
                    row["marker_seen"] = (row["marker_seen"] and (case_root / "download.bpk").is_file()
                                          and (case_root / "download.bpk").read_bytes() == b"fixture-package")
            row["status"] = "pass" if result.returncode == 0 and not row["control_bytes"] and row["marker_seen"] else "fail"
        template = root / "local-template"
        (template / ".beskid").mkdir(parents=True)
        (template / "Src").mkdir()
        (template / ".beskid" / "template.json").write_text(json.dumps({
            "schema": "beskid.template.v1", "identity": "cli-gate", "name": "CLI gate",
            "shortName": "cli-gate", "tags": {"type": "project"}, "sources": [{}],
            "symbols": {"name": {"type": "string", "isRequired": True}}, "postActions": [],
        }))
        (template / "{{name}}.bproj").write_text(
            '{{name}} { name = "{{name}}" version = "0.1.0" }\n'
            'target "{{name}}" { kind = "App" entry = "Smoke.bd" }\n')
        (template / "Src" / "Smoke.bd").write_text(source.read_text())
        (template / "{{name}}.txt").write_text("{{name}}\n")
        generated = root / "generated"
        new_args = ["new", "--path", str(template), "--name", "MatrixSmoke", "--no-interactive", "-o", str(generated)]
        new_result = run(binary, new_args, env, root)
        new_row = {"path": "new <local-template>", "kind": "scenario", "argv": new_args,
                   "expected_exit": 0, "exit": new_result.returncode}
        new_row.update(output_evidence(new_result.stdout, new_result.stderr))
        new_row["generated_manifest"] = (generated / "MatrixSmoke.bproj").is_file()
        new_row["substituted_output"] = ((generated / "MatrixSmoke.txt").read_text() == "MatrixSmoke\n"
                                         if (generated / "MatrixSmoke.txt").is_file() else False)
        new_row["status"] = "pass" if (new_result.returncode == 0 and not new_row["control_bytes"]
                                        and new_row["generated_manifest"] and new_row["substituted_output"]) else "fail"
        rows.append(new_row)

        graph_file = root / "graph.mmd"
        graph_file_args = ["graph", "--project", str(project), "--plain", "--out", str(graph_file)]
        graph_file_result = run(binary, graph_file_args, env, root)
        graph_file_row = {"path": "graph --out", "kind": "scenario", "argv": graph_file_args,
                          "expected_exit": 0, "exit": graph_file_result.returncode}
        graph_file_row.update(output_evidence(graph_file_result.stdout, graph_file_result.stderr))
        graph_file_row["mermaid_file"] = graph_file.is_file() and "flowchart" in graph_file.read_text()
        graph_file_row["status"] = "pass" if (graph_file_result.returncode == 0 and not graph_file_row["control_bytes"]
                                               and graph_file_row["mermaid_file"]) else "fail"
        rows.append(graph_file_row)

        graph_tui_args = ["graph", "--project", str(project), "--plain", "--tui"]
        graph_tui = run_pty(binary, graph_tui_args, dict(env, TERM="xterm-256color"), root)
        graph_tui_bytes = base64.b64decode(graph_tui["transcript_base64"])
        graph_tui_row = {"path": "graph --tui", "kind": "scenario", "argv": graph_tui_args,
                         "expected_exit": 0, **graph_tui}
        graph_tui_row["entered_alternate_screen"] = b"\x1b[?1049h" in graph_tui_bytes
        graph_tui_row["rendered_project"] = graph_tui_rendered(graph_tui_bytes)
        graph_tui_row["status"] = "pass" if (graph_tui["exit"] == 0 and not graph_tui["timed_out"]
                                              and graph_tui_row["rendered_project"]) else "fail"
        rows.append(graph_tui_row)

        analyze_pty_args = ["analyze", "--plain", str(source)]
        analyze_pty = run_pty(binary, analyze_pty_args, dict(env, TERM="xterm-256color"), root)
        analyze_pty_bytes = base64.b64decode(analyze_pty["transcript_base64"])
        analyze_pty_row = {"path": "analyze --plain PTY", "kind": "scenario", "argv": analyze_pty_args,
                           "expected_exit": 0, **analyze_pty}
        analyze_pty_row["line_output"] = ordinary_pty_clean(analyze_pty_bytes)
        analyze_pty_row["summary_seen"] = b"Analyze complete" in analyze_pty_bytes
        analyze_pty_row["status"] = "pass" if (analyze_pty["exit"] == 0 and not analyze_pty["timed_out"]
                                                and analyze_pty_row["line_output"]
                                                and analyze_pty_row["summary_seen"]) else "fail"
        rows.append(analyze_pty_row)

        version_result = run(binary, ["--version"], env, root)
        version_row = {"path": "--version", "kind": "scenario", "argv": ["--version"],
                       "expected_exit": 0, "exit": version_result.returncode}
        version_row.update(output_evidence(version_result.stdout, version_result.stderr))
        version_row["status"] = "pass" if (version_result.returncode == 0 and not version_row["control_bytes"]
                                           and b"beskid" in version_result.stdout.lower()) else "fail"
        rows.append(version_row)

        failures = release_failures(rows)
        new_help = run(binary, ["new", "--help"], env, root)
        graph_help = run(binary, ["graph", "--help"], env, root)
        removed = run(binary, ["hi"], env, root)
        removed_picker = run(binary, ["new", "--tui"], env, root)
        contracts = {
            "hi_unknown": {"expected_exit": 2, "exit": removed.returncode,
                           "unknown_subcommand": (b"unrecognized subcommand" in removed.stderr
                                                  or b"unknown subcommand" in removed.stderr),
                           **output_evidence(removed.stdout, removed.stderr)},
            "new_tui_rejected": {"expected_exit": 2, "exit": removed_picker.returncode,
                                 "not_advertised": b"--tui" not in new_help.stdout,
                                 "unexpected_argument": b"unexpected argument '--tui'" in removed_picker.stderr,
                                 **output_evidence(removed_picker.stdout, removed_picker.stderr)},
            "graph_tui_advertised": b"--tui" in graph_help.stdout,
        }
        contract_ok = (contracts["hi_unknown"]["exit"] == 2 and contracts["hi_unknown"]["unknown_subcommand"]
                       and not contracts["hi_unknown"]["control_bytes"]
                       and contracts["new_tui_rejected"]["exit"] == 2
                       and contracts["new_tui_rejected"]["not_advertised"]
                       and contracts["new_tui_rejected"]["unexpected_argument"]
                       and not contracts["new_tui_rejected"]["control_bytes"]
                       and contracts["graph_tui_advertised"])
        if not contract_ok:
            failures.append("hi/new/graph command contract")
        counts = {status: sum(row["status"] == status for row in rows)
                  for status in ("pass", "fail", "setup_skip", "uncovered", "inventory_only")}
        evidence = {"schema": "beskid.cli-surface.v1", "binary": str(binary),
                    "binary_sha256": binary_sha256,
                    "source_provenance": source_provenance(), "release_qualified": False,
                    "counts": counts,
                    "contracts": contracts, "rows": rows}
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(evidence, indent=2) + "\n")
        registry_server.shutdown()
        registry_server.server_close()
        print(json.dumps({"counts": counts, "failures": failures, "evidence": str(args.json)}))
        return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
