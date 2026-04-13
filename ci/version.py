"""Cargo.toml version helpers for beskid_cli."""

from __future__ import annotations

import re
import tomllib
from pathlib import Path

CARGO_TOML = Path("crates/beskid_cli/Cargo.toml")


def _read_text() -> str:
    return CARGO_TOML.read_text(encoding="utf-8")


def _write_text(content: str) -> None:
    CARGO_TOML.write_text(content, encoding="utf-8")


def set_package_version(version: str) -> None:
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


def bump_patch_version() -> None:
    content = _read_text()
    match = re.search(r'(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"\s*$', content)
    if not match:
        raise RuntimeError(f"unable to parse semver in {CARGO_TOML}")
    major, minor, patch = map(int, match.groups())
    next_version = f"{major}.{minor}.{patch + 1}"
    new_content = (
        content[: match.start()] + f'version = "{next_version}"' + content[match.end() :]
    )
    _write_text(new_content)


def read_package_version() -> str:
    data = tomllib.loads(_read_text())
    return str(data["package"]["version"])
