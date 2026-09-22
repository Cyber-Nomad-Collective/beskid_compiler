"""Single authority for native ABI-v5 runtime-kit evidence coordinates."""

from __future__ import annotations

SCHEMA_VERSION = 4
PROFILES = ("debug", "release")
LINKAGES = ("static", "shared")

# Consumers that exercise the runtime-kit boundary. The CLI remains debug-only;
# every direct executable lifecycle witness links the staged static runtime.
CONSUMER_LINKAGES = {
    "jit": "shared",
    "aot": "static",
    "repl": "shared",
    "cli": "shared",
    "native-executable-streams": "static",
    "native-executable-stdin": "static",
    "native-executable-stdout": "static",
    "native-executable-stderr": "static",
    "native-executable-args": "static",
    "native-executable-unit-entry": "static",
}

CONSUMER_SUCCESS_EXIT_CODES = {
    consumer: 42 if consumer == "cli" else 0
    for consumer in CONSUMER_LINKAGES
}

DIRECT_LIFECYCLE_CONSUMERS = (
    "native-executable-streams",
    "native-executable-stdin",
    "native-executable-stdout",
    "native-executable-stderr",
    "native-executable-args",
    "native-executable-unit-entry",
)


def smoke_coordinates() -> dict[tuple[str, str], str]:
    """Return every required `(profile, consumer)` evidence coordinate."""
    coordinates = {
        (profile, consumer): CONSUMER_LINKAGES[consumer]
        for profile in PROFILES
        for consumer in ("jit", "aot", "repl", *DIRECT_LIFECYCLE_CONSUMERS)
    }
    coordinates[("debug", "cli")] = CONSUMER_LINKAGES["cli"]
    return coordinates


def consumers_for(profile: str, linkage: str) -> tuple[str, ...]:
    """Return the required consumers for one profile/linkage cell."""
    return tuple(
        consumer
        for (coordinate_profile, consumer), coordinate_linkage in smoke_coordinates().items()
        if coordinate_profile == profile and coordinate_linkage == linkage
    )


def success_exit_code(profile: str, consumer: str) -> int:
    """Return the required successful exit status for one smoke coordinate."""
    if (profile, consumer) not in smoke_coordinates():
        raise ValueError(f"unknown smoke coordinate: {profile}/{consumer}")
    return CONSUMER_SUCCESS_EXIT_CODES[consumer]


SMOKES_PER_TARGET = len(smoke_coordinates())
AGGREGATE_TARGET_COUNT = 3
AGGREGATE_SMOKE_COUNT = SMOKES_PER_TARGET * AGGREGATE_TARGET_COUNT
