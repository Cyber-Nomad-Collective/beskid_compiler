"""Nox tasks for the Beskid compiler repository."""

from __future__ import annotations

import os
import sys
from pathlib import Path

import nox

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))

_ASAN = {
    # Keep sanitizer flags target-scoped so host proc-macro deps remain loadable.
    "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS": "-Zsanitizer=address",
    "RUSTC_BOOTSTRAP": "1",
    "ASAN_OPTIONS": "detect_leaks=0",
}


def _cargo(session: nox.Session, *args: str, env: dict[str, str] | None = None) -> None:
    merged = {**os.environ, **(env or {})}
    with session.chdir(str(ROOT)):
        session.run("cargo", *args, external=True, env=merged)


@nox.session(python=False, name="workspace_check")
def workspace_check(session: nox.Session) -> None:
    _cargo(session, "check", "--workspace")


@nox.session(python=False)
def test(session: nox.Session) -> None:
    _cargo(session, "test", "-p", "beskid_tests")


@nox.session(python=False, name="abi_contracts")
def abi_contracts(session: nox.Session) -> None:
    _cargo(session, "test", "-p", "beskid_tests", "abi::contracts::")


@nox.session(python=False, name="bench_compile")
def bench_compile(session: nox.Session) -> None:
    _cargo(session, "bench", "-p", "beskid_runtime", "--no-run")


@nox.session(python=False, name="e2e_linux")
def e2e_linux(session: nox.Session) -> None:
    _cargo(session, "build", "-p", "beskid_cli")
    _cargo(session, "test", "-p", "beskid_e2e_tests")


@nox.session(python=False, name="runtime_asan_linux")
def runtime_asan_linux(session: nox.Session) -> None:
    _cargo(
        session,
        "test",
        "-p",
        "beskid_tests",
        "--target",
        "x86_64-unknown-linux-gnu",
        "runtime::",
        env=_ASAN,
    )


@nox.session(python=False, name="extern_engine_security")
def extern_engine_security(session: nox.Session) -> None:
    _cargo(
        session,
        "test",
        "-p",
        "beskid_engine",
        "--features",
        "extern_dlopen",
        "security_allow_deny_sequences",
    )


@nox.session(python=False, name="e2e_macos_smoke")
def e2e_macos_smoke(session: nox.Session) -> None:
    _cargo(session, "build", "-p", "beskid_cli")
    _cargo(session, "test", "-p", "beskid_e2e_tests", "cli_cross_platform")


@nox.session(python=False, name="e2e_windows_smoke")
def e2e_windows_smoke(session: nox.Session) -> None:
    _cargo(session, "build", "-p", "beskid_cli")
    _cargo(session, "test", "-p", "beskid_e2e_tests", "cli_cross_platform")


@nox.session(python="3.12", name="compute_version")
def compute_version(session: nox.Session) -> None:
    with session.chdir(str(ROOT)):
        session.run("python", "-m", "ci.version_job")


@nox.session(python="3.12", name="release_cli")
def release_cli(session: nox.Session) -> None:
    session.install("-r", str(ROOT / "ci" / "requirements.txt"))
    with session.chdir(str(ROOT)):
        session.run("python", "-m", "ci.release_cli")
