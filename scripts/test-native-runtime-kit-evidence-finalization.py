#!/usr/bin/env python3
"""Regression coverage for producer-native runtime-kit smoke finalization."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import pathlib
import tempfile
from contextlib import contextmanager

from native_runtime_kit_evidence_contract import (
    DIRECT_LIFECYCLE_CONSUMERS,
    LINKAGES,
    PROFILES,
    SCHEMA_VERSION,
    consumers_for,
    smoke_coordinates,
    success_exit_code,
)

SCRIPT = pathlib.Path(__file__).with_name("native-runtime-kit-evidence.py")
spec = importlib.util.spec_from_file_location("producer_evidence", SCRIPT)
assert spec and spec.loader
producer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(producer)

TARGET = "x86_64-unknown-linux-gnu"
ARTIFACTS = {
    "static": "static/libbeskid_runtime.a",
    "shared": "shared/libbeskid_runtime.so",
}


def write(path: pathlib.Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(value, bytes):
        path.write_bytes(value)
    else:
        path.write_text(json.dumps(value) + "\n", encoding="utf-8")


@contextmanager
def evidence_directory(path: pathlib.Path):
    previous = os.environ.get("BESKID_RUNTIME_KIT_EVIDENCE_DIR")
    os.environ["BESKID_RUNTIME_KIT_EVIDENCE_DIR"] = str(path)
    try:
        yield
    finally:
        if previous is None:
            del os.environ["BESKID_RUNTIME_KIT_EVIDENCE_DIR"]
        else:
            os.environ["BESKID_RUNTIME_KIT_EVIDENCE_DIR"] = previous


def fixture(root: pathlib.Path, omit: tuple[str, str] | None = None) -> tuple[pathlib.Path, pathlib.Path]:
    evidence = root / "evidence"
    runtime = root / "runtime"
    write(evidence / "producer.json", {"revisions": {}, "source_identities": {}})
    for profile in PROFILES:
        artifacts = {}
        for linkage in LINKAGES:
            relative = ARTIFACTS[linkage]
            artifact = runtime / profile / relative
            write(artifact, f"{profile}/{linkage}\n".encode())
            artifacts[f"{linkage}_library"] = {
                "relative_path": relative,
                "sha256": producer.sha256(artifact),
            }
            for report in (
                f"symbols/raw/{profile}-{linkage}-defined.txt",
                f"symbols/raw/{profile}-{linkage}-undefined.txt",
                f"symbols/normalized/{profile}-{linkage}.symbols",
            ):
                write(evidence / report, f"{report}\n".encode())
        artifacts["shared_import_library"] = None
        write(
            runtime / profile / "abi.json",
            {
                "schema_version": 1,
                "abi_version": 5,
                "target": {"triple": TARGET},
                "profile": profile,
                "layout_hash": "layout",
                "source_hash": "source",
                "artifacts": artifacts,
            },
        )
    for (profile, consumer), linkage in smoke_coordinates().items():
        if (profile, consumer) == omit:
            continue
        output_path = f"smokes/{profile}-{consumer}.log"
        write(evidence / output_path, f"{profile}/{consumer}\n".encode())
        write(
            evidence / "smokes" / f"{profile}-{consumer}.json",
            {
                "schema_version": SCHEMA_VERSION,
                "target": TARGET,
                "profile": profile,
                "consumer": consumer,
                "linkage_boundary": linkage,
                "status": "passed",
                "exit_code": success_exit_code(profile, consumer),
                "output_path": output_path,
            },
        )
    return evidence, runtime


def finalize(evidence: pathlib.Path, runtime: pathlib.Path) -> None:
    with evidence_directory(evidence):
        producer.command_finish(
            argparse.Namespace(
                status="passed",
                exit_code=0,
                target=TARGET,
                runtime_root=str(runtime),
            )
        )


def main() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = pathlib.Path(temporary)
        evidence, runtime = fixture(root)
        finalize(evidence, runtime)
        for profile in PROFILES:
            static = json.loads(
                (evidence / "cells" / f"{profile}-static.json").read_text(encoding="utf-8")
            )
            shared = json.loads(
                (evidence / "cells" / f"{profile}-shared.json").read_text(encoding="utf-8")
            )
            assert {smoke["consumer"] for smoke in static["smokes"]} == set(
                consumers_for(profile, "static")
            )
            assert set(DIRECT_LIFECYCLE_CONSUMERS) <= {
                smoke["consumer"] for smoke in static["smokes"]
            }
            assert not set(DIRECT_LIFECYCLE_CONSUMERS) & {
                smoke["consumer"] for smoke in shared["smokes"]
            }

        root = pathlib.Path(temporary) / "incomplete"
        evidence, runtime = fixture(root, omit=("release", "native-executable-unit-entry"))
        try:
            finalize(evidence, runtime)
        except RuntimeError as error:
            assert "smoke cardinality mismatch" in str(error)
        else:
            raise AssertionError("producer accepted an incomplete lifecycle smoke set")


if __name__ == "__main__":
    main()
