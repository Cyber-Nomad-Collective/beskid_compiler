# Compiler workspace — common test tasks.
#
#   just corelib    Run corelib_tests via beskid_cli
#   just compiler   Run cargo test for the workspace
#   just tests      Run compiler and corelib tests

set shell := ["bash", "-euo", "pipefail", "-c"]

root := justfile_directory()

corelib_tests_project := "corelib/beskid_corelib/tests/corelib_tests"

default:
    @just --list

# Run every corelib_tests target via beskid_cli (shared Salsa session).
corelib:
    cargo run -p beskid_cli --quiet -- test \
        --project "{{corelib_tests_project}}" \
        --all-targets \
        --plain

# Run the full compiler workspace test suite.
compiler:
    cargo test

# Run compiler and corelib tests.
tests: compiler corelib
