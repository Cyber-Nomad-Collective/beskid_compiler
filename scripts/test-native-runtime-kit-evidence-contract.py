#!/usr/bin/env python3
"""Focused regression checks for native runtime-kit evidence coordinates."""

from __future__ import annotations

from native_runtime_kit_evidence_contract import (
    AGGREGATE_SMOKE_COUNT,
    CONSUMER_LINKAGES,
    CONSUMER_SUCCESS_EXIT_CODES,
    DIRECT_LIFECYCLE_CONSUMERS,
    PROFILES,
    SCHEMA_VERSION,
    SMOKES_PER_TARGET,
    consumers_for,
    smoke_coordinates,
    success_exit_code,
)


def main() -> None:
    coordinates = smoke_coordinates()
    assert SCHEMA_VERSION == 4
    assert PROFILES == ("debug", "release")
    assert DIRECT_LIFECYCLE_CONSUMERS == (
        "native-executable-streams",
        "native-executable-stdin",
        "native-executable-stdout",
        "native-executable-stderr",
        "native-executable-args",
        "native-executable-unit-entry",
    )
    assert CONSUMER_LINKAGES == {
        "jit": "shared",
        "aot": "static",
        "repl": "shared",
        "cli": "shared",
        **{consumer: "static" for consumer in DIRECT_LIFECYCLE_CONSUMERS},
    }
    assert CONSUMER_SUCCESS_EXIT_CODES == {
        consumer: 42 if consumer == "cli" else 0
        for consumer in CONSUMER_LINKAGES
    }
    assert all(CONSUMER_LINKAGES[consumer] == "static" for consumer in DIRECT_LIFECYCLE_CONSUMERS)
    assert set(coordinates) == {
        (profile, consumer)
        for profile in PROFILES
        for consumer in ("jit", "aot", "repl", *DIRECT_LIFECYCLE_CONSUMERS)
    } | {("debug", "cli")}
    assert coordinates[("debug", "cli")] == "shared"
    assert success_exit_code("debug", "cli") == 42
    assert all(
        success_exit_code(profile, consumer) == 0
        for profile, consumer in coordinates
        if consumer != "cli"
    )
    assert "cli" not in (consumer for profile, consumer in coordinates if profile == "release")
    assert set(DIRECT_LIFECYCLE_CONSUMERS) <= set(consumers_for("debug", "static"))
    assert set(DIRECT_LIFECYCLE_CONSUMERS) <= set(consumers_for("release", "static"))
    assert SMOKES_PER_TARGET == 19
    assert AGGREGATE_SMOKE_COUNT == 57


if __name__ == "__main__":
    main()
