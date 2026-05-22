"""Compute CLI version for GitHub Actions ``version`` job (no repo writes)."""

from __future__ import annotations

import os

from ci import github_output
from ci import version as ver


def main() -> None:
    event = os.environ.get("GITHUB_EVENT_NAME", "")
    if event != "push":
        raise SystemExit(f"compute_version expects GITHUB_EVENT_NAME=push, got {event!r}")

    ref = os.environ.get("GITHUB_REF", "")
    if not (ref.startswith("refs/tags/v") or ref == "refs/heads/main"):
        raise SystemExit(f"Unexpected GITHUB_REF for version job: {ref!r}")

    out = ver.resolve_version()
    github_output.write_output("version", out)


if __name__ == "__main__":
    main()
