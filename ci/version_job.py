"""Compute and optionally bump CLI version for GitHub Actions `version` job."""

from __future__ import annotations

import os

from ci import github_output
from ci import version as ver


def main() -> None:
    ref = os.environ.get("GITHUB_REF", "")
    event = os.environ.get("GITHUB_EVENT_NAME", "")
    ref_name = os.environ.get("GITHUB_REF_NAME", "")

    if event != "push":
        raise SystemExit(f"compute_version expects GITHUB_EVENT_NAME=push, got {event!r}")
    if ref.startswith("refs/tags/v"):
        tag_version = ref_name[1:] if ref_name.startswith("v") else ref_name
        ver.set_package_version(tag_version)
    elif ref == "refs/heads/main":
        ver.bump_patch_version()
    else:
        raise SystemExit(f"Unexpected GITHUB_REF for version job: {ref!r}")

    out = ver.read_package_version()
    github_output.write_output("version", out)


if __name__ == "__main__":
    main()
