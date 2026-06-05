# Compiler workspace — common test tasks.
#
#   just corelib    Run corelib_tests via release beskid_cli
#   just compiler   Run cargo test for the workspace
#   just tests      Run compiler and corelib tests

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
