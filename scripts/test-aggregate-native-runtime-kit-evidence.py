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
            "superproject_sha": "1" * 40,
            "compiler_sha": "2" * 40,
            "corelib_sha": "3" * 40,
            "bsol_sha": "4" * 40,
            "superproject_dirty": False,
            "compiler_dirty": False,
            "corelib_dirty": False,
            "bsol_dirty": False,
            "gitlinks": {
                "compiler": "2" * 40,
                "compiler/corelib": "3" * 40,
                "beskid_bsol": "4" * 40,
            },
        }
        write(
            evidence / "producer.json",
            {"schema_version": aggregate.SCHEMA_VERSION, "target": target, "revisions": revisions},
        )
        write(
            evidence / "result.json",
            {"schema_version": aggregate.SCHEMA_VERSION, "target": target, "status": "passed", "revisions": revisions},
        )

        smoke_records = {}
        for (profile, consumer), linkage in smoke_coordinates.items():
            output_path = f"smokes/{profile}-{consumer}.log"
            write(evidence / output_path, f"{target}/{profile}/{consumer}\n".encode())
            record = {
                "schema_version": aggregate.SCHEMA_VERSION,
                "target": target,
                "profile": profile,
                "consumer": consumer,
                "linkage_boundary": linkage,
                "status": "passed",
                "exit_code": aggregate.success_exit_code(profile, consumer),
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
                    "schema_version": aggregate.SCHEMA_VERSION,
                    "target": target,
                    "profile": profile,
                    "status": "passed",
                    "revisions": revisions,
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
                        "schema_version": aggregate.SCHEMA_VERSION,
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
        argparse.Namespace(download_root=str(root), expected_root_sha="1" * 40)
    )
    assert result["status"] == "passed"
    assert result["cardinality"]["linkage_cells"] == 12
    assert result["cardinality"]["smokes"] == 57
    assert result["corelib_sha"] == "3" * 40
    assert result["bsol_sha"] == "4" * 40


def expect_failure(root: pathlib.Path, message: str, error_contains: str = "") -> None:
    try:
        verify(root)
    except RuntimeError as error:
        if error_contains and error_contains not in str(error):
            raise AssertionError(f"{message}: unexpected failure: {error}") from error
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

        producer_path = root / "abi-v5-runtime-kit-evidence-linux-x86_64/producer.json"
        for repository in ("superproject", "compiler", "corelib", "bsol"):
            for invalid in (True, 0, "false", None):
                fixture(root)
                producer = json.loads(producer_path.read_text(encoding="utf-8"))
                producer["revisions"][f"{repository}_dirty"] = invalid
                write(producer_path, producer)
                expect_failure(root, f"aggregate accepted {repository} dirty fact {invalid!r}")
            for invalid in ("", "not-a-commit", None, 123, [], "a" * 39):
                fixture(root)
                producer = json.loads(producer_path.read_text(encoding="utf-8"))
                producer["revisions"][f"{repository}_sha"] = invalid
                write(producer_path, producer)
                expect_failure(root, f"aggregate accepted malformed {repository} revision {invalid!r}")
            fixture(root)
            producer = json.loads(producer_path.read_text(encoding="utf-8"))
            del producer["revisions"][f"{repository}_sha"]
            write(producer_path, producer)
            expect_failure(root, f"aggregate accepted missing {repository} revision")

        for path, repository in (("compiler", "compiler"), ("compiler/corelib", "corelib"), ("beskid_bsol", "bsol")):
            fixture(root)
            producer = json.loads(producer_path.read_text(encoding="utf-8"))
            producer["revisions"]["gitlinks"][path] = "a" * 40
            write(producer_path, producer)
            expect_failure(root, f"aggregate accepted {path} gitlink/HEAD mismatch")

            fixture(root)
            producer = json.loads(producer_path.read_text(encoding="utf-8"))
            producer["revisions"][f"{repository}_sha"] = "a" * 40
            producer["revisions"]["gitlinks"][path] = "a" * 40
            write(producer_path, producer)
            for relative in ("result.json", "profiles/debug/result.json", "profiles/release/result.json"):
                receipt_path = producer_path.parent / relative
                receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
                receipt["revisions"] = producer["revisions"]
                write(receipt_path, receipt)
            expect_failure(root, f"aggregate accepted different {repository} revisions across targets", "platform evidence full source closure revisions do not agree")

        fixture(root)
        producer = json.loads(producer_path.read_text(encoding="utf-8"))
        del producer["revisions"]["gitlinks"]
        write(producer_path, producer)
        expect_failure(root, "aggregate accepted missing source gitlinks")

        fixture(root)
        producer = json.loads(producer_path.read_text(encoding="utf-8"))
        producer["schema_version"] = 3
        write(producer_path, producer)
        expect_failure(root, "aggregate accepted legacy v3 evidence")

        fixture(root)
        missing_lifecycle = root / "abi-v5-runtime-kit-evidence-linux-x86_64/smokes/debug-native-executable-streams.json"
        missing_lifecycle.unlink()
        expect_failure(root, "aggregate accepted a missing lifecycle smoke receipt")

        fixture(root)
        substituted_lifecycle = root / "abi-v5-runtime-kit-evidence-linux-x86_64/smokes/debug-native-executable-streams.json"
        lifecycle = json.loads(substituted_lifecycle.read_text(encoding="utf-8"))
        lifecycle["consumer"] = "native-executable-stdin"
        write(substituted_lifecycle, lifecycle)
        expect_failure(root, "aggregate accepted a substituted lifecycle consumer")

        fixture(root)
        wrong_linkage = root / "abi-v5-runtime-kit-evidence-linux-x86_64/smokes/debug-native-executable-streams.json"
        lifecycle = json.loads(wrong_linkage.read_text(encoding="utf-8"))
        lifecycle["linkage_boundary"] = "shared"
        write(wrong_linkage, lifecycle)
        expect_failure(root, "aggregate accepted a lifecycle smoke with shared linkage")

        fixture(root)
        static_cell = root / "abi-v5-runtime-kit-evidence-linux-x86_64/cells/debug-static.json"
        cell = json.loads(static_cell.read_text(encoding="utf-8"))
        for smoke in cell["smokes"]:
            if smoke["consumer"] == "native-executable-streams":
                smoke["output_sha256"] = "0" * 64
                break
        else:
            raise AssertionError("fixture omitted debug static lifecycle smoke")
        write(static_cell, cell)
        expect_failure(root, "aggregate accepted a lifecycle smoke output hash mismatch")

        for relative in ("result.json", "profiles/debug/result.json", "profiles/release/result.json"):
            for field, value in (("bsol_dirty", True), ("bsol_sha", "a" * 40)):
                fixture(root)
                path = producer_path.parent / relative
                record = json.loads(path.read_text(encoding="utf-8"))
                record["revisions"][field] = value
                if field.endswith("_sha"):
                    record["revisions"]["gitlinks"]["beskid_bsol"] = value
                write(path, record)
                expect_failure(root, f"aggregate accepted {relative} divergent closure {field}")

    print("Native runtime-kit aggregate evidence tests OK")


if __name__ == "__main__":
    main()
