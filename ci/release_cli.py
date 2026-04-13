"""Build and upload CLI binaries (release-cli matrix job)."""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

from ci import s3_upload
from ci import version as ver


def _require(name: str) -> str:
    value = os.environ.get(name, "").strip()
    if not value:
        raise SystemExit(f"Missing required environment variable: {name}")
    return value


def main() -> None:
    root = Path.cwd()
    release_version = _require("RELEASE_VERSION")
    target = _require("MATRIX_TARGET")
    asset_name = _require("MATRIX_ASSET_NAME")
    runner_os = os.environ.get("RUNNER_OS", "")

    ver.set_package_version(release_version)

    subprocess.run(
        [
            "cargo",
            "build",
            "-p",
            "beskid_cli",
            "--release",
            "--target",
            target,
        ],
        check=True,
        cwd=root,
    )

    if runner_os == "Windows":
        built = root / "target" / target / "release" / "beskid_cli.exe"
    else:
        built = root / "target" / target / "release" / "beskid_cli"
    if not built.is_file():
        raise SystemExit(f"Expected binary at {built}")

    dest = root / asset_name
    shutil.move(str(built), str(dest))

    access_key = _require("SEAWEEDFS_ACCESS_KEY")
    secret_key = _require("SEAWEEDFS_SECRET_KEY")
    endpoint = os.environ.get("SEAWEEDFS_ENDPOINT", "https://cdn.beskid-lang.org")

    s3_upload.upload_release_artifacts(
        endpoint_url=endpoint,
        access_key_id=access_key,
        secret_access_key=secret_key,
        bucket="releases",
        region="us-east-1",
        version=release_version,
        local_file=dest,
        remote_filename=asset_name,
    )


if __name__ == "__main__":
    main()
