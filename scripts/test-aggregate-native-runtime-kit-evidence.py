#!/usr/bin/env python3
"""Regression tests for exact aggregate evidence coordinates and rooted paths."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import pathlib
import tempfile

SCRIPT = pathlib.Path(__file__).with_name("aggregate-native-runtime-kit-evidence.py")
spec = importlib.util.spec_from_file_location("aggregate_evidence", SCRIPT)
assert spec and spec.loader
aggregate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(aggregate)


def write(path: pathlib.Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(value, bytes):
        path.write_bytes(value)
    else:
        path.write_text(json.dumps(value) + "\n", encoding="utf-8")


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fixture(root: pathlib.Path) -> None:
    smoke_coordinates = aggregate.expected_smokes()
    for label, target in aggregate.TARGETS.items():
        evidence = root / f"abi-v5-runtime-kit-evidence-{label}"
        kit = root / f"abi-v5-runtime-kit-{label}"
        revisions = {
            "superproject_sha": "root-sha",
            "compiler_sha": "compiler-sha",
            "compiler_dirty": False,
        }
        write(
            evidence / "producer.json",
            {"schema_version": 2, "target": target, "revisions": revisions},
        )
        write(
            evidence / "result.json",
            {"schema_version": 2, "target": target, "status": "passed"},
        )

        smoke_records = {}
        for (profile, consumer), linkage in smoke_coordinates.items():
            output_path = f"smokes/{profile}-{consumer}.log"
            write(evidence / output_path, f"{target}/{profile}/{consumer}\n".encode())
            record = {
                "schema_version": 2,
                "target": target,
                "profile": profile,
                "consumer": consumer,
                "linkage_boundary": linkage,
                "status": "passed",
                "exit_code": 0,
                "output_path": output_path,
            }
            write(evidence / "smokes" / f"{profile}-{consumer}.json", record)
            smoke_records[(profile, consumer)] = record

        for profile in aggregate.PROFILES:
            metadata = kit / profile / "abi.json"
            write(metadata, b"{}\n")
            write(
                evidence / "profiles" / profile / "result.json",
                {
                    "schema_version": 2,
                    "target": target,
                    "profile": profile,
                    "status": "passed",
                },
            )
            for linkage in aggregate.LINKAGES:
                artifact_name = aggregate.ARTIFACTS[target][linkage]
                artifact_path = f"{profile}/{linkage}/{artifact_name}"
                artifact = kit / artifact_path
                write(artifact, f"{target}/{profile}/{linkage}\n".encode())
                verifier = {"status": "passed"}
                for key, report_path in {
                    "raw_defined_report": f"symbols/raw/{profile}-{linkage}-defined.txt",
                    "raw_undefined_report": f"symbols/raw/{profile}-{linkage}-undefined.txt",
                    "symbol_report": f"symbols/normalized/{profile}-{linkage}.symbols",
                }.items():
                    report = evidence / report_path
                    write(report, f"{key}\n".encode())
                    verifier[key] = report_path
                    verifier[f"{key}_sha256"] = digest(report)
                cell_smokes = []
                for (smoke_profile, consumer), boundary in smoke_coordinates.items():
                    if smoke_profile == profile and boundary == linkage:
                        smoke = dict(smoke_records[(profile, consumer)])
                        smoke["output_sha256"] = digest(evidence / smoke["output_path"])
                        cell_smokes.append(smoke)
                import_record = None
                if target == "x86_64-pc-windows-msvc" and linkage == "shared":
                    import_path = f"{profile}/shared/beskid_runtime_import.lib"
                    imported = kit / import_path
                    write(imported, f"import/{profile}\n".encode())
                    import_record = {"path": import_path, "sha256": digest(imported)}
                write(
                    evidence / "cells" / f"{profile}-{linkage}.json",
                    {
                        "schema_version": 2,
                        "target": target,
                        "profile": profile,
                        "linkage": linkage,
                        "status": "passed",
                        "artifact": {"path": artifact_path, "sha256": digest(artifact)},
                        "metadata": {
                            "path": f"{profile}/abi.json",
                            "sha256": digest(metadata),
                        },
                        "verifier": verifier,
                        "smokes": cell_smokes,
                        "import_library": import_record,
                    },
                )


def verify(root: pathlib.Path) -> None:
    result = aggregate.verify(
        argparse.Namespace(download_root=str(root), expected_root_sha="root-sha")
    )
    assert result["status"] == "passed"
    assert result["cardinality"]["linkage_cells"] == 12


def expect_failure(root: pathlib.Path, message: str) -> None:
    try:
        verify(root)
    except RuntimeError:
        return
    raise AssertionError(message)


def main() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = pathlib.Path(temporary)
        fixture(root)
        verify(root)

        cell_path = (
            root / "abi-v5-runtime-kit-evidence-linux-x86_64/cells/debug-static.json"
        )
        cell = json.loads(cell_path.read_text(encoding="utf-8"))
        cell["profile"] = "release"
        write(cell_path, cell)
        expect_failure(root, "aggregate accepted a substituted cell coordinate")

        fixture(root)
        cell = json.loads(cell_path.read_text(encoding="utf-8"))
        cell["artifact"]["path"] = (
            "../abi-v5-runtime-kit-linux-x86_64/debug/static/libbeskid_runtime.a"
        )
        write(cell_path, cell)
        expect_failure(root, "aggregate accepted artifact path traversal")

    print("Native runtime-kit aggregate evidence tests OK")


if __name__ == "__main__":
    main()
