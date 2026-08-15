#!/usr/bin/env python3
"""Fail-closed aggregate verifier for downloaded native ABI-v5 kit evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys
from typing import Any

SCHEMA_VERSION = 2
TARGETS = {
    "windows-x86_64": "x86_64-pc-windows-msvc",
    "linux-x86_64": "x86_64-unknown-linux-gnu",
    "macos-arm64": "aarch64-apple-darwin",
}
PROFILES = ("debug", "release")
LINKAGES = ("static", "shared")
ARTIFACTS = {
    "x86_64-unknown-linux-gnu": {
        "static": "libbeskid_runtime.a",
        "shared": "libbeskid_runtime.so",
    },
    "aarch64-apple-darwin": {
        "static": "libbeskid_runtime.a",
        "shared": "libbeskid_runtime.dylib",
    },
    "x86_64-pc-windows-msvc": {
        "static": "beskid_runtime.lib",
        "shared": "beskid_runtime.dll",
    },
}
SMOKE_LINKAGES = {"jit": "shared", "aot": "static", "repl": "shared", "cli": "shared"}


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load(path: pathlib.Path) -> dict[str, Any]:
    if not path.is_file() or path.is_symlink():
        raise RuntimeError(f"missing regular JSON evidence file: {path}")
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise RuntimeError(f"evidence JSON must contain an object: {path}")
    return value


def write(path: pathlib.Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def require_fields(
    record: dict[str, Any], expected: dict[str, Any], description: str
) -> None:
    for key, value in expected.items():
        if record.get(key) != value:
            raise RuntimeError(
                f"{description} has invalid {key}: expected {value!r}, got {record.get(key)!r}"
            )


def safe_file(root: pathlib.Path, relative: Any, description: str) -> pathlib.Path:
    if not isinstance(relative, str) or not relative:
        raise RuntimeError(f"{description} path must be a non-empty relative string")
    candidate = pathlib.PurePosixPath(relative)
    if candidate.is_absolute() or ".." in candidate.parts or "." in candidate.parts:
        raise RuntimeError(
            f"{description} path escapes its artifact root: {relative!r}"
        )
    path = root.joinpath(*candidate.parts)
    try:
        resolved = path.resolve(strict=True)
        resolved.relative_to(root)
    except (FileNotFoundError, ValueError) as error:
        raise RuntimeError(
            f"{description} is outside or missing from its artifact root: {relative!r}"
        ) from error
    if not resolved.is_file() or path.is_symlink() or resolved != path.absolute():
        raise RuntimeError(
            f"{description} must be a regular non-symlink file: {relative!r}"
        )
    return resolved


def verify_hash(
    root: pathlib.Path, record: dict[str, Any], expected_path: str, description: str
) -> None:
    if record.get("path") != expected_path:
        raise RuntimeError(
            f"{description} path mismatch: expected {expected_path!r}, got {record.get('path')!r}"
        )
    path = safe_file(root, expected_path, description)
    if sha256(path) != record.get("sha256"):
        raise RuntimeError(f"{description} hash mismatch")


def expected_smokes() -> dict[tuple[str, str], str]:
    coordinates = {
        (profile, consumer): SMOKE_LINKAGES[consumer]
        for profile in PROFILES
        for consumer in ("jit", "aot", "repl")
    }
    coordinates[("debug", "cli")] = "shared"
    return coordinates


def verify_smoke(
    evidence: pathlib.Path,
    smoke: dict[str, Any],
    target: str,
    profile: str,
    consumer: str,
    linkage: str,
    require_hash: bool,
) -> None:
    require_fields(
        smoke,
        {
            "schema_version": SCHEMA_VERSION,
            "target": target,
            "profile": profile,
            "consumer": consumer,
            "linkage_boundary": linkage,
            "status": "passed",
            "exit_code": 0,
            "output_path": f"smokes/{profile}-{consumer}.log",
        },
        f"smoke {target}/{profile}/{consumer}",
    )
    output = safe_file(
        evidence, smoke["output_path"], f"smoke output {target}/{profile}/{consumer}"
    )
    if require_hash and sha256(output) != smoke.get("output_sha256"):
        raise RuntimeError(f"smoke output hash mismatch: {target}/{profile}/{consumer}")


def verify(args: argparse.Namespace) -> dict[str, Any]:
    root = pathlib.Path(args.download_root).resolve(strict=True)
    revisions: set[str] = set()
    compiler_revisions: set[str] = set()
    cells = profiles = smokes = imports = 0
    target_results = []
    smoke_coordinates = expected_smokes()

    for label, target in TARGETS.items():
        evidence = (root / f"abi-v5-runtime-kit-evidence-{label}").resolve(strict=True)
        kit = (root / f"abi-v5-runtime-kit-{label}").resolve(strict=True)
        evidence.relative_to(root)
        kit.relative_to(root)

        summary = load(evidence / "result.json")
        producer = load(evidence / "producer.json")
        require_fields(
            summary,
            {"schema_version": SCHEMA_VERSION, "status": "passed", "target": target},
            f"target summary {target}",
        )
        require_fields(
            producer,
            {"schema_version": SCHEMA_VERSION, "target": target},
            f"producer {target}",
        )
        revision = producer.get("revisions")
        if (
            not isinstance(revision, dict)
            or revision.get("compiler_dirty") is not False
        ):
            raise RuntimeError(f"dirty or malformed compiler evidence for {target}")
        revisions.add(revision.get("superproject_sha", ""))
        compiler_revisions.add(revision.get("compiler_sha", ""))

        expected_cell_names = {
            f"{profile}-{linkage}.json" for profile in PROFILES for linkage in LINKAGES
        }
        actual_cell_names = {
            path.name
            for path in (evidence / "cells").iterdir()
            if path.suffix == ".json"
        }
        if actual_cell_names != expected_cell_names:
            raise RuntimeError(
                f"cell cardinality mismatch for {target}: expected {sorted(expected_cell_names)}, "
                f"got {sorted(actual_cell_names)}"
            )

        nested_smokes: dict[tuple[str, str], dict[str, Any]] = {}
        seen_cells: set[tuple[str, str, str]] = set()
        for profile in PROFILES:
            profile_result = load(evidence / "profiles" / profile / "result.json")
            require_fields(
                profile_result,
                {
                    "schema_version": SCHEMA_VERSION,
                    "status": "passed",
                    "target": target,
                    "profile": profile,
                },
                f"profile {target}/{profile}",
            )
            profiles += 1
            for linkage in LINKAGES:
                coordinate = (target, profile, linkage)
                if coordinate in seen_cells:
                    raise RuntimeError(
                        f"duplicate verifier cell: {target}/{profile}/{linkage}"
                    )
                seen_cells.add(coordinate)
                cell = load(evidence / "cells" / f"{profile}-{linkage}.json")
                require_fields(
                    cell,
                    {
                        "schema_version": SCHEMA_VERSION,
                        "status": "passed",
                        "target": target,
                        "profile": profile,
                        "linkage": linkage,
                    },
                    f"cell {target}/{profile}/{linkage}",
                )
                verifier = cell.get("verifier")
                if not isinstance(verifier, dict) or verifier.get("status") != "passed":
                    raise RuntimeError(
                        f"failed verifier cell for {target}/{profile}/{linkage}"
                    )

                artifact_path = f"{profile}/{linkage}/{ARTIFACTS[target][linkage]}"
                verify_hash(
                    kit,
                    cell.get("artifact", {}),
                    artifact_path,
                    f"artifact {target}/{profile}/{linkage}",
                )
                verify_hash(
                    kit,
                    cell.get("metadata", {}),
                    f"{profile}/abi.json",
                    f"metadata {target}/{profile}/{linkage}",
                )

                report_paths = {
                    "raw_defined_report": f"symbols/raw/{profile}-{linkage}-defined.txt",
                    "raw_undefined_report": f"symbols/raw/{profile}-{linkage}-undefined.txt",
                    "symbol_report": f"symbols/normalized/{profile}-{linkage}.symbols",
                }
                for key, expected_path in report_paths.items():
                    if verifier.get(key) != expected_path:
                        raise RuntimeError(
                            f"symbol report path mismatch: {target}/{profile}/{linkage}/{key}"
                        )
                    report = safe_file(
                        evidence,
                        expected_path,
                        f"symbol report {target}/{profile}/{linkage}/{key}",
                    )
                    if sha256(report) != verifier.get(f"{key}_sha256"):
                        raise RuntimeError(
                            f"symbol report hash mismatch: {target}/{profile}/{linkage}/{key}"
                        )

                expected_consumers = {
                    consumer
                    for (p, consumer), boundary in smoke_coordinates.items()
                    if p == profile and boundary == linkage
                }
                cell_smokes = cell.get("smokes")
                if not isinstance(cell_smokes, list):
                    raise RuntimeError(
                        f"cell smokes must be a list: {target}/{profile}/{linkage}"
                    )
                actual_consumers: set[str] = set()
                for smoke in cell_smokes:
                    if not isinstance(smoke, dict):
                        raise RuntimeError(
                            f"malformed smoke record: {target}/{profile}/{linkage}"
                        )
                    consumer = smoke.get("consumer")
                    if consumer in actual_consumers or not isinstance(consumer, str):
                        raise RuntimeError(
                            f"duplicate or malformed smoke coordinate: {target}/{profile}/{linkage}/{consumer}"
                        )
                    actual_consumers.add(consumer)
                    verify_smoke(
                        evidence, smoke, target, profile, consumer, linkage, True
                    )
                    nested_smokes[(profile, consumer)] = smoke
                if actual_consumers != expected_consumers:
                    raise RuntimeError(
                        f"cell smoke coordinate mismatch for {target}/{profile}/{linkage}: "
                        f"expected {sorted(expected_consumers)}, got {sorted(actual_consumers)}"
                    )

                import_record = cell.get("import_library")
                if target == "x86_64-pc-windows-msvc" and linkage == "shared":
                    if not isinstance(import_record, dict):
                        raise RuntimeError(
                            f"missing Windows import-library evidence: {profile}"
                        )
                    verify_hash(
                        kit,
                        import_record,
                        f"{profile}/shared/beskid_runtime_import.lib",
                        f"Windows import library {profile}",
                    )
                    imports += 1
                elif import_record is not None:
                    raise RuntimeError(
                        f"unexpected import-library evidence: {target}/{profile}/{linkage}"
                    )
                cells += 1

        if set(nested_smokes) != set(smoke_coordinates):
            raise RuntimeError(f"nested smoke coordinate mismatch for {target}")
        expected_names = {
            f"{profile}-{consumer}.json" for profile, consumer in smoke_coordinates
        }
        smoke_dir = evidence / "smokes"
        actual_names = {
            path.name for path in smoke_dir.iterdir() if path.suffix == ".json"
        }
        if actual_names != expected_names:
            raise RuntimeError(f"smoke cardinality mismatch for {target}")
        for (profile, consumer), linkage in smoke_coordinates.items():
            standalone = load(smoke_dir / f"{profile}-{consumer}.json")
            verify_smoke(
                evidence, standalone, target, profile, consumer, linkage, False
            )
            nested = nested_smokes[(profile, consumer)]
            for key in (
                "target",
                "profile",
                "consumer",
                "linkage_boundary",
                "status",
                "exit_code",
                "output_path",
            ):
                if standalone.get(key) != nested.get(key):
                    raise RuntimeError(
                        f"standalone/nested smoke mismatch: {target}/{profile}/{consumer}/{key}"
                    )
        smokes += len(actual_names)
        target_results.append({"target": target, "status": "passed"})

    if (
        "" in revisions
        or "" in compiler_revisions
        or len(revisions) != 1
        or len(compiler_revisions) != 1
    ):
        raise RuntimeError("platform evidence revisions do not agree")
    if args.expected_root_sha and revisions != {args.expected_root_sha}:
        raise RuntimeError("evidence root revision does not match workflow revision")
    if (profiles, cells, imports, smokes) != (6, 12, 2, 21):
        raise RuntimeError(
            f"aggregate cardinality mismatch: profiles={profiles}, cells={cells}, imports={imports}, smokes={smokes}"
        )
    return {
        "schema_version": 1,
        "status": "passed",
        "root_sha": next(iter(revisions)),
        "compiler_sha": next(iter(compiler_revisions)),
        "cardinality": {
            "targets": 3,
            "profiles": profiles,
            "linkage_cells": cells,
            "windows_import_libraries": imports,
            "smokes": smokes,
        },
        "targets": target_results,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("download_root")
    parser.add_argument("output")
    parser.add_argument("--expected-root-sha", default="")
    args = parser.parse_args()
    output = pathlib.Path(args.output)
    try:
        result = verify(args)
    except Exception as error:
        write(output, {"schema_version": 1, "status": "failed", "error": str(error)})
        print(f"native runtime-kit aggregate error: {error}", file=sys.stderr)
        return 1
    write(output, result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
