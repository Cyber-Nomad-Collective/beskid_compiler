#!/usr/bin/env python3
"""Onboarding check for a clean machine (0.6.7 ticket 15, part 3).

Runs in a fresh temporary HOME so nothing from the developer machine leaks in:
  1. install the toolchain from a verified release bundle with scripts/install-local-toolchain-bundle.sh
  2. `beskid doctor`
  3. for every project/workspace template of `beskid new --list` (or --templates):
     `beskid new`, then `beskid check`, `beskid build`, `beskid test`, `beskid fmt`, `beskid fmt --check`
  4. optionally (--soak PATH) a short language-server pass over the generated projects, with the
     installed `beskid_lsp`, failing on any crash report: "no editor errors"

A step fails on a nonzero exit, a timeout,
a signal, or an internal-error signature (an E21xx internal diagnostic, E4041, panic text, ...).
The editor extension install itself is the one manual step on a fresh macOS user; this script covers
everything the extension relies on.

usage: onboarding_check.py --bundle DIR --version X.Y.Z [options]
  --bundle DIR        verified local release bundle (bin/, lib/beskid-runtime, beskid_corelib, release-version.txt)
  --version X.Y.Z     release version, must equal release-version.txt
  --installer PATH    install script (default: scripts/install-local-toolchain-bundle.sh of this checkout)
  --templates LIST    comma list of template short names (default: all project/workspace templates)
  --soak PATH         beskid-soak binary; adds the language-server pass
  --soak-seconds N    length of that pass (default 30)
  --timeout SECONDS   per command (default 900)
  --keep              keep the temporary HOME
  --out FILE          JSON report path
Exit status: 0 all steps passed, 1 a step failed, 2 usage/setup error.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _proof_common as common  # noqa: E402

# `beskid test` on a project without tests prints exactly this line and exits 0 (like `cargo test`).
NO_TESTS = re.compile(r"^No tests found\.$", re.MULTILINE)


def tool_dirs():
    """Directories of bash and a C compiler as resolved on the caller's PATH, so the restricted
    PATH below also works where they are not in /usr/bin (the NixOS builder)."""
    dirs = []
    for tool in ("bash", "cc", "ld"):
        found = shutil.which(tool)
        if found:
            directory = str(Path(found).parent)
            if directory not in dirs:
                dirs.append(directory)
    return dirs


def restricted_path(prefix):
    parts = [str(prefix / "bin"), *tool_dirs(), "/usr/bin", "/bin", "/usr/sbin", "/sbin"]
    return os.pathsep.join(dict.fromkeys(parts))


def step_status(name, result, patterns):
    signatures = common.classify(result.output, result.returncode, result.timed_out, patterns)
    if name == "test" and result.returncode == 0 and not signatures and NO_TESTS.search(result.output):
        return "ok (no tests)", []
    if signatures:
        return "FAIL", signatures
    return ("ok" if result.returncode == 0 else "FAIL"), []


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bundle", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--installer", default=str(Path(__file__).resolve().parents[1] / "install-local-toolchain-bundle.sh"))
    parser.add_argument("--templates")
    parser.add_argument("--soak")
    parser.add_argument("--soak-seconds", type=int, default=30)
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--out")
    args = parser.parse_args(argv)

    home = Path(tempfile.mkdtemp(prefix="beskid-onboarding-home-"))
    prefix = home / "toolchain"
    env = {
        "HOME": str(home),
        "USERPROFILE": str(home),
        "PATH": restricted_path(prefix),
        "LANG": os.environ.get("LANG", "C.UTF-8"),
        "TMPDIR": str(home / "tmp"),
    }
    (home / "tmp").mkdir()
    patterns = common.ice_patterns()
    steps = []  # (scope, step, status, detail)

    def record(scope, step, status, result=None, signatures=()):
        detail = ""
        if result is not None and status.startswith("FAIL"):
            detail = (", ".join(signatures) + "\n" if signatures else "") + common.excerpt(result.output, patterns)
        steps.append({"scope": scope, "step": step, "status": status, "detail": detail,
                      "seconds": round(result.seconds, 1) if result else 0})
        print(f"[onboarding] {scope}: {step}: {status}", file=sys.stderr)

    install = common.run([shutil.which("bash") or "bash", args.installer, args.bundle, args.version, prefix], timeout=args.timeout, env=env)
    record("toolchain", "install", "ok" if install.returncode == 0 else "FAIL", install)
    beskid = str(prefix / "bin" / "beskid")
    if install.returncode != 0 or not Path(beskid).exists():
        return finish(args, steps, home, 1)

    doctor = common.run([beskid, "doctor"], cwd=home, timeout=args.timeout, env=env)
    status, signatures = step_status("doctor", doctor, patterns)
    record("toolchain", "doctor", status, doctor, signatures)

    available = [name for name, _ in common.discover_templates(beskid, env)[0]]
    chosen = [t for t in args.templates.split(",") if t] if args.templates else available
    if not chosen:
        record("templates", "discover", "FAIL")
        return finish(args, steps, home, 1)

    projects_dir = home / "projects"
    projects_dir.mkdir()
    for short in chosen:
        name = "onboard_" + short.replace("-", "_")
        project = projects_dir / name
        created = common.run(
            [beskid, "new", name, "--template", short, "--offline", "--no-interactive", "-o", project],
            cwd=projects_dir, timeout=args.timeout, env=env,
        )
        status, signatures = step_status("new", created, patterns)
        record(short, "new", status, created, signatures)
        if status != "ok":
            continue
        for step, command in (
            ("check", ["check"]),
            ("build", ["build"]),
            ("test", ["test"]),
            ("fmt", ["fmt", str(project)]),
            ("fmt --check", ["fmt", "--check", str(project)]),
        ):
            command = [beskid, *command]
            if step in ("check", "build", "test"):
                command += ["--project", str(project)]
            result = common.run(command, cwd=project, timeout=args.timeout, env=env)
            status, signatures = step_status(step.split()[0], result, patterns)
            record(short, step, status, result, signatures)

    if args.soak:
        soak = common.run(
            [args.soak, "--server", str(prefix / "bin" / "beskid_lsp"), "--corpus", str(projects_dir),
             "--duration", f"{args.soak_seconds}s", "--home", str(home), "--beskid", beskid],
            timeout=args.soak_seconds + 600, env=env,
        )
        record("editor", "language-server pass", "ok" if soak.returncode == 0 else "FAIL", soak)
        if soak.returncode != 0:
            steps[-1]["detail"] = soak.output[-600:]

    return finish(args, steps, home, 0)


def finish(args, steps, home, base_status):
    failed = [s for s in steps if s["status"].startswith("FAIL")]
    print()
    for scope in dict.fromkeys(s["scope"] for s in steps):
        row = ", ".join(f"{s['step']} {s['status']}" for s in steps if s["scope"] == scope)
        print(f"{scope}: {row}")
    for s in failed:
        print(f"FAILED {s['scope']} / {s['step']}:\n  " + s["detail"].replace("\n", "\n  "))
    print(f"RESULT: {'FAIL' if failed or base_status else 'PASS'} ({len(steps)} steps, {len(failed)} failed)")
    if args.out:
        Path(args.out).write_text(json.dumps({"steps": steps, "failed": len(failed)}, indent=2))
    if args.keep or failed:
        print(f"[onboarding] temporary HOME kept: {home}", file=sys.stderr)
    else:
        shutil.rmtree(home, ignore_errors=True)
    return 1 if failed or base_status else 0


if __name__ == "__main__":
    sys.exit(main())
