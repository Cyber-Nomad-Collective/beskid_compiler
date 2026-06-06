# Compiler workspace — common test tasks.
#
#   just corelib    Run corelib_tests via release beskid_cli
#   just compiler   Run cargo test for the workspace
#   just tests      Run compiler and corelib tests
#   just replace    Build release CLI and overwrite installed `beskid`

set shell := ["bash", "-euo", "pipefail", "-c"]

root := justfile_directory()

corelib_tests_project := "corelib/beskid_corelib/tests/corelib_tests"

default:
    @just --list

# Run every corelib_tests target via release beskid_cli (shared Salsa session).
corelib:
    cargo build -p beskid_cli --release --quiet
    "{{root}}/target/release/beskid_cli" test \
        --project "{{corelib_tests_project}}" \
        --all-targets

# Run the full compiler workspace test suite.
compiler:
    cargo test

# Run compiler and corelib tests.
tests: compiler corelib

# Build release beskid_cli and replace the installed `beskid` binary.
replace:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{root}}"
    cargo build -p beskid_cli --release
    built="{{root}}/target/release/beskid_cli"
    dest="$(command -v beskid 2>/dev/null || true)"
    if [[ -z "${dest}" ]]; then
      dest="${HOME}/.beskid/bin/beskid"
    fi
    mkdir -p "$(dirname "${dest}")"
    install -m 0755 "${built}" "${dest}"
    echo "Replaced ${dest}"
