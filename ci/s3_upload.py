"""Upload release binaries to S3-compatible storage (e.g. SeaweedFS)."""

from __future__ import annotations

import time
from pathlib import Path

import boto3
from botocore.exceptions import ClientError, ConnectionClosedError, EndpointConnectionError, SSLError
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
    version_key = f"{version}/{remote_filename}"
    latest_key = f"latest/{remote_filename}"
    _put_object(client=client, local_file=local_file, bucket=bucket, key=version_key)
    _put_object(client=client, local_file=local_file, bucket=bucket, key=latest_key)


def _put_object(*, client: "boto3.client", local_file: Path, bucket: str, key: str) -> None:
    """Upload as a single-part object to avoid multipart/chunked edge cases."""
    # SeaweedFS gateways can terminate TLS on streamed/chunked payload uploads.
    # Sending explicit bytes with ContentLength avoids chunked transfer mode.
    payload = local_file.read_bytes()
    max_attempts = 4

    for attempt in range(1, max_attempts + 1):
        try:
            client.put_object(
                Bucket=bucket,
                Key=key,
                Body=payload,
                ContentLength=len(payload),
                ContentType="application/octet-stream",
            )
            return
        except (SSLError, EndpointConnectionError, ConnectionClosedError) as exc:
            if attempt >= max_attempts:
                raise RuntimeError(
                    "S3 upload failed after retries for "
                    f"s3://{bucket}/{key} due to transport/TLS error: {exc}"
                ) from exc
            time.sleep(2 ** (attempt - 1))
        except ClientError as exc:
            code = exc.response.get("Error", {}).get("Code", "Unknown")
            msg = exc.response.get("Error", {}).get("Message", str(exc))
            raise RuntimeError(
                "S3 upload failed for "
                f"s3://{bucket}/{key} "
                f"(error={code}: {msg}). "
                "Check SeaweedFS credentials and bucket policy for PutObject on "
                f"the '{key}' prefix."
            ) from exc
