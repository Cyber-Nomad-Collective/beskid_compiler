"""Resolve and apply beskid_cli semver for CI release builds.

Rolling main builds derive a monotonic patch from the latest ``v*`` git tag plus
commits since that tag (same model as ``compiler/corelib/ci/version.py``). When
no semver tags exist yet, CI uses ``GITHUB_RUN_NUMBER`` on top of the Cargo.toml
base so published ``cli-version.txt`` does not stick on a single patch forever.
"""

from __future__ import annotations

import os
import re
import subprocess
import tomllib
from pathlib import Path

CARGO_TOML = Path("crates/beskid_cli/Cargo.toml")
SEMVER_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
TAG_RE = re.compile(r"^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")


def _read_text() -> str:
    return CARGO_TOML.read_text(encoding="utf-8")


def _write_text(content: str) -> None:
    CARGO_TOML.write_text(content, encoding="utf-8")


def set_package_version(version: str) -> None:
    """Patch ``beskid_cli`` Cargo.toml in the build workspace (not committed)."""
    if not SEMVER_RE.match(version):
        raise ValueError(f"invalid semver for beskid_cli: {version!r}")
    content = _read_text()
    content, count = re.subn(
        r'(?m)^version\s*=\s*"[^"]+"\s*$',
        f'version = "{version}"',
        content,
        count=1,
    )
    if count != 1:
        raise RuntimeError(f"failed to update version in {CARGO_TOML}")
    _write_text(content)


def read_package_version() -> str:
    data = tomllib.loads(_read_text())
    return str(data["package"]["version"])


def cli_release_tag(version: str) -> str:
    """Immutable GitHub release tag for a resolved CLI semver."""
    return f"cli-v{version}"


def _parse_semver(version: str) -> tuple[int, int, int]:
    match = SEMVER_RE.match(version.strip())
    if not match:
        raise ValueError(f"invalid semver: {version!r}")
    return int(match.group(1)), int(match.group(2)), int(match.group(3))


def _git(*args: str) -> str:
    return subprocess.check_output(["git", *args], text=True).strip()


def _latest_semver_tag() -> str | None:
    try:
        return subprocess.check_output(
            [
                "git",
                "describe",
                "--tags",
                "--abbrev=0",
                "--match",
                "v[0-9]*.[0-9]*.[0-9]*",
            ],
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except subprocess.CalledProcessError:
        return None


def _commits_since_tag(tag: str) -> int:
    return int(_git("rev-list", "--count", f"{tag}..HEAD"))


def _version_from_tag(ref: str, ref_name: str) -> str | None:
    if not ref.startswith("refs/tags/"):
        return None
    tag = ref_name.strip()
    match = TAG_RE.match(tag)
    if not match:
        raise SystemExit(
            f"Tag {tag!r} is not semver (expected vMAJOR.MINOR.PATCH, e.g. v0.1.0)",
        )
    return tag.removeprefix("v")


def resolve_version(
    *,
    github_ref: str = "",
    github_ref_name: str = "",
    github_run_number: str = "",
) -> str:
    """Compute the CLI version string for the current CI context."""
    ref = github_ref or os.environ.get("GITHUB_REF", "")
    ref_name = github_ref_name or os.environ.get("GITHUB_REF_NAME", "")
    run_number = github_run_number or os.environ.get("GITHUB_RUN_NUMBER", "")

    tagged = _version_from_tag(ref, ref_name)
    if tagged is not None:
        return tagged

    if ref and ref != "refs/heads/main":
        raise SystemExit(f"Unexpected GITHUB_REF for version resolution: {ref!r}")

    base = read_package_version()
    major, minor, patch = _parse_semver(base)

    latest_tag = _latest_semver_tag()
    if latest_tag:
        tag_match = TAG_RE.match(latest_tag)
        if not tag_match:
            raise SystemExit(f"Latest tag {latest_tag!r} is not semver")
        t_major, t_minor, t_patch = (int(tag_match.group(i)) for i in range(1, 4))
        commits_since = _commits_since_tag(latest_tag)
        if commits_since <= 0:
            return f"{t_major}.{t_minor}.{t_patch}"
        return f"{t_major}.{t_minor}.{t_patch + commits_since}"

    if run_number.strip().isdigit():
        return f"{major}.{minor}.{patch + int(run_number)}"

    return base
