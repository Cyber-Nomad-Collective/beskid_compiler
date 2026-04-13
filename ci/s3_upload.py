"""Upload release binaries to S3-compatible storage (e.g. SeaweedFS)."""

from __future__ import annotations

from pathlib import Path

import boto3
from botocore.client import Config


def upload_release_artifacts(
    *,
    endpoint_url: str,
    access_key_id: str,
    secret_access_key: str,
    bucket: str,
    region: str,
    version: str,
    local_file: Path,
    remote_filename: str,
) -> None:
    client = boto3.client(
        "s3",
        endpoint_url=endpoint_url,
        aws_access_key_id=access_key_id,
        aws_secret_access_key=secret_access_key,
        region_name=region,
        config=Config(signature_version="s3v4", s3={"addressing_style": "path"}),
    )
    version_key = f"releases/{version}/{remote_filename}"
    latest_key = f"releases/latest/{remote_filename}"
    client.upload_file(str(local_file), bucket, version_key)
    client.upload_file(str(local_file), bucket, latest_key)
