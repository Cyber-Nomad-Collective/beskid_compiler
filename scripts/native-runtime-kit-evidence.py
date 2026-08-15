#!/usr/bin/env python3
"""Create and validate machine-readable native ABI-v5 runtime-kit evidence."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import pathlib
import platform
import subprocess
import sys
from typing import Any

SCHEMA_VERSION = 2
PROFILES = ("debug", "release")
LINKAGES = ("static", "shared")
EXPECTED_ARTIFACTS = {
    "x86_64-unknown-linux-gnu": (
        "static/libbeskid_runtime.a",
        "shared/libbeskid_runtime.so",
        None,
    ),
    "aarch64-apple-darwin": (
        "static/libbeskid_runtime.a",
        "shared/libbeskid_runtime.dylib",
        None,
    ),
    "x86_64-pc-windows-msvc": (
        "static/beskid_runtime.lib",
        "shared/beskid_runtime.dll",
        "shared/beskid_runtime_import.lib",
    ),
}


def utc_now() -> str:
    return (
        dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git(root: pathlib.Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args], check=True, text=True, capture_output=True
    )
    return result.stdout.strip()


def write_json(path: pathlib.Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    temporary.replace(path)


def evidence_dir() -> pathlib.Path:
    return pathlib.Path(os.environ["BESKID_RUNTIME_KIT_EVIDENCE_DIR"]).resolve()


def command_init(args: argparse.Namespace) -> None:
    output = evidence_dir()
    output.mkdir(parents=True, exist_ok=True)
    compiler_root = pathlib.Path(args.compiler_root).resolve()
    superproject_root = compiler_root.parent
    dirty = bool(git(compiler_root, "status", "--porcelain"))
    if dirty:
        raise RuntimeError(
            "native runtime-kit evidence rejects a dirty compiler checkout"
        )
    openspec_catalog = superproject_root / "openspec" / "catalog.json"
    producer = {
        "schema_version": SCHEMA_VERSION,
        "target": args.target,
        "started_at_utc": utc_now(),
        "revisions": {
            "superproject_sha": git(superproject_root, "rev-parse", "HEAD"),
            "compiler_sha": git(compiler_root, "rev-parse", "HEAD"),
            "github_sha": os.environ.get("GITHUB_SHA", ""),
            "compiler_dirty": dirty,
        },
        "source_identities": {
            "runtime_manifest_sha256": sha256(compiler_root / "runtime_manifest.bsol"),
            "openspec_catalog_sha256": sha256(openspec_catalog)
            if openspec_catalog.is_file()
            else None,
        },
        "host": {
            "platform": platform.platform(),
            "runner_os": os.environ.get("RUNNER_OS", ""),
            "runner_arch": os.environ.get("RUNNER_ARCH", ""),
            "runner_image_os": os.environ.get("ImageOS", ""),
            "runner_image_version": os.environ.get("ImageVersion", ""),
        },
        "tools": {
            "python": sys.version.splitlines()[0],
            "rustc": subprocess.run(
                ["rustc", "--version", "--verbose"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout,
            "symbol_tool": str(pathlib.Path(args.symbol_tool).resolve()),
            "symbol_tool_version": subprocess.run(
                [args.symbol_tool, "--version"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout,
        },
        "github": {
            key.lower(): os.environ.get(key, "")
            for key in (
                "GITHUB_REPOSITORY",
                "GITHUB_WORKFLOW",
                "GITHUB_RUN_ID",
                "GITHUB_RUN_ATTEMPT",
                "GITHUB_JOB",
            )
        },
    }
    write_json(output / "producer.json", producer)
    write_json(
        output / "result.json",
        {
            "schema_version": SCHEMA_VERSION,
            "target": args.target,
            "status": "running",
            "started_at_utc": producer["started_at_utc"],
        },
    )


def command_smoke(args: argparse.Namespace) -> None:
    if args.status not in ("passed", "failed"):
        raise ValueError("smoke status must be passed or failed")
    record = {
        "schema_version": SCHEMA_VERSION,
        "target": args.target,
        "profile": args.profile,
        "consumer": args.consumer,
        "linkage_boundary": args.linkage,
        "status": args.status,
        "exit_code": args.exit_code,
        "command": args.command,
        "output_path": args.output_path,
        "completed_at_utc": utc_now(),
    }
    write_json(
        evidence_dir() / "smokes" / f"{args.profile}-{args.consumer}.json", record
    )


def require_regular(path: pathlib.Path) -> None:
    if not path.is_file() or path.is_symlink():
        raise RuntimeError(f"required evidence artifact is not a regular file: {path}")


def validate_metadata(
    root: pathlib.Path, target: str, profile: str
) -> tuple[dict[str, Any], dict[str, Any]]:
    metadata_path = root / profile / "abi.json"
    require_regular(metadata_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    if metadata.get("schema_version") != 1 or metadata.get("abi_version") != 5:
        raise RuntimeError(f"invalid ABI-v5 metadata version: {metadata_path}")
    actual_target = metadata.get("target", {}).get("triple")
    if actual_target != target or metadata.get("profile") != profile:
        raise RuntimeError(f"metadata coordinate mismatch: {metadata_path}")
    expected_static, expected_shared, expected_import = EXPECTED_ARTIFACTS[target]
    artifacts = metadata.get("artifacts", {})
    coordinates = {
        "static_library": expected_static,
        "shared_library": expected_shared,
        "shared_import_library": expected_import,
    }
    verified: dict[str, Any] = {}
    for key, expected_relative in coordinates.items():
        entry = artifacts.get(key)
        if expected_relative is None:
            if entry is not None:
                raise RuntimeError(f"unexpected {key} in {metadata_path}")
            verified[key] = None
            continue
        if (
            not isinstance(entry, dict)
            or entry.get("relative_path") != expected_relative
        ):
            raise RuntimeError(f"invalid {key} coordinate in {metadata_path}")
        artifact_path = root / profile / expected_relative
        require_regular(artifact_path)
        digest = sha256(artifact_path)
        if digest != entry.get("sha256"):
            raise RuntimeError(f"metadata hash mismatch for {artifact_path}")
        verified[key] = {"path": f"{profile}/{expected_relative}", "sha256": digest}
    return metadata, verified


def write_hash_inventory(root: pathlib.Path, output: pathlib.Path) -> None:
    records = []
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise RuntimeError(f"runtime kit contains forbidden symlink: {path}")
        if path.is_file():
            records.append(f"{sha256(path)}  {path.relative_to(root).as_posix()}")
    output.write_text("\n".join(records) + ("\n" if records else ""), encoding="utf-8")


def command_finish(args: argparse.Namespace) -> None:
    output = evidence_dir()
    producer_path = output / "producer.json"
    producer = (
        json.loads(producer_path.read_text(encoding="utf-8"))
        if producer_path.is_file()
        else None
    )
    result = {
        "schema_version": SCHEMA_VERSION,
        "target": args.target,
        "status": args.status,
        "exit_code": args.exit_code,
        "completed_at_utc": utc_now(),
        "cell_count": 0,
        "smoke_count": len(list((output / "smokes").glob("*.json")))
        if (output / "smokes").is_dir()
        else 0,
    }
    if args.status == "passed":
        if producer is None:
            raise RuntimeError(
                "successful evidence finalization requires producer.json"
            )
        runtime_root = pathlib.Path(args.runtime_root).resolve()
        cells = []
        identities = set()
        for profile in PROFILES:
            metadata, artifacts = validate_metadata(runtime_root, args.target, profile)
            identities.add((metadata["layout_hash"], metadata["source_hash"]))
            profile_cells = []
            for linkage in LINKAGES:
                artifact_key = f"{linkage}_library"
                cell = {
                    "schema_version": SCHEMA_VERSION,
                    "target": args.target,
                    "profile": profile,
                    "linkage": linkage,
                    "status": "passed",
                    "artifact": artifacts[artifact_key],
                    "metadata": {
                        "status": "passed",
                        "path": f"{profile}/abi.json",
                        "sha256": sha256(runtime_root / profile / "abi.json"),
                        "layout_hash": metadata["layout_hash"],
                        "source_hash": metadata["source_hash"],
                    },
                    "verifier": {
                        "status": "passed",
                        "policy": f"canonical {linkage} runtime provenance policy",
                        "raw_defined_report": f"symbols/raw/{profile}-{linkage}-defined.txt",
                        "raw_undefined_report": f"symbols/raw/{profile}-{linkage}-undefined.txt",
                        "symbol_report": f"symbols/normalized/{profile}-{linkage}.symbols",
                    },
                    "import_library": artifacts["shared_import_library"]
                    if linkage == "shared"
                    else None,
                }
                for report_key in (
                    "raw_defined_report",
                    "raw_undefined_report",
                    "symbol_report",
                ):
                    report = output / cell["verifier"][report_key]
                    require_regular(report)
                    cell["verifier"][f"{report_key}_sha256"] = sha256(report)
                consumers = ("aot",) if linkage == "static" else ("jit", "repl", "cli")
                cell["smokes"] = []
                for consumer in consumers:
                    smoke_path = output / "smokes" / f"{profile}-{consumer}.json"
                    if smoke_path.is_file():
                        smoke = json.loads(smoke_path.read_text(encoding="utf-8"))
                        smoke_log = output / smoke["output_path"]
                        require_regular(smoke_log)
                        smoke["output_sha256"] = sha256(smoke_log)
                        cell["smokes"].append(smoke)
                write_json(output / "cells" / f"{profile}-{linkage}.json", cell)
                cells.append(cell)
                profile_cells.append(cell)
            profile_smokes = []
            for smoke_path in sorted((output / "smokes").glob(f"{profile}-*.json")):
                profile_smokes.append(
                    json.loads(smoke_path.read_text(encoding="utf-8"))
                )
            write_json(
                output / "profiles" / profile / "result.json",
                {
                    "schema_version": SCHEMA_VERSION,
                    "target": args.target,
                    "profile": profile,
                    "status": "passed",
                    "revisions": producer["revisions"],
                    "source_identities": producer["source_identities"],
                    "metadata": {
                        "status": "passed",
                        "path": f"{profile}/abi.json",
                        "sha256": sha256(runtime_root / profile / "abi.json"),
                        "layout_hash": metadata["layout_hash"],
                        "source_hash": metadata["source_hash"],
                    },
                    "cells": profile_cells,
                    "smokes": profile_smokes,
                },
            )
        if len(identities) != 1:
            raise RuntimeError("debug and release metadata identities differ")
        expected_smokes = {
            f"{profile}-{consumer}.json"
            for profile in PROFILES
            for consumer in ("jit", "aot", "repl")
        }
        expected_smokes.add("debug-cli.json")
        actual_smokes = {path.name for path in (output / "smokes").glob("*.json")}
        if actual_smokes != expected_smokes:
            raise RuntimeError(
                f"smoke cardinality mismatch: expected {sorted(expected_smokes)}, got {sorted(actual_smokes)}"
            )
        for path in (output / "smokes").glob("*.json"):
            smoke = json.loads(path.read_text(encoding="utf-8"))
            if smoke.get("status") != "passed" or smoke.get("target") != args.target:
                raise RuntimeError(f"failed or mismatched smoke record: {path}")
        write_hash_inventory(runtime_root, output / "kit-sha256.txt")
        write_hash_inventory(output / "symbols", output / "symbol-report-sha256.txt")
        result["cell_count"] = len(cells)
        result["smoke_count"] = len(actual_smokes)
        result["revisions"] = producer["revisions"]
        result["source_identities"] = producer["source_identities"]
        result["metadata_identity"] = {
            "layout_hash": next(iter(identities))[0],
            "source_hash": next(iter(identities))[1],
        }
    write_json(output / "result.json", result)


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="operation", required=True)
    init = commands.add_parser("init")
    init.add_argument("target")
    init.add_argument("symbol_tool")
    init.add_argument("compiler_root")
    init.set_defaults(function=command_init)
    smoke = commands.add_parser("smoke")
    smoke.add_argument("target")
    smoke.add_argument("profile", choices=PROFILES)
    smoke.add_argument("consumer", choices=("jit", "aot", "repl", "cli"))
    smoke.add_argument("linkage")
    smoke.add_argument("status")
    smoke.add_argument("exit_code", type=int)
    smoke.add_argument("output_path")
    smoke.add_argument("command")
    smoke.set_defaults(function=command_smoke)
    finish = commands.add_parser("finish")
    finish.add_argument("status", choices=("passed", "failed"))
    finish.add_argument("exit_code", type=int)
    finish.add_argument("target")
    finish.add_argument("runtime_root")
    finish.set_defaults(function=command_finish)
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        args.function(args)
    except Exception as error:
        print(f"native runtime-kit evidence error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
