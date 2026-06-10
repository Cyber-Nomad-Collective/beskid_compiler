# Compiler workspace — common test tasks.
#
#   just corelib    Run corelib_tests via release beskid_cli
#   just compiler   Run cargo test for the workspace
#   just tests      Run compiler and corelib tests
#   just replace    Build release CLI + LSP and overwrite installed `beskid` / `beskid_lsp`

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

# Build release beskid_cli + beskid_lsp and replace installed toolchain binaries.
replace:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{root}}"
    cargo build -p beskid_cli -p beskid_lsp --release
    cli_built="{{root}}/target/release/beskid_cli"
    lsp_built="{{root}}/target/release/beskid_lsp"
    cli_dest="$(command -v beskid 2>/dev/null || true)"
    if [[ -z "${cli_dest}" ]]; then
      cli_dest="${HOME}/.beskid/bin/beskid"
    fi
    lsp_dest="$(command -v beskid_lsp 2>/dev/null || true)"
    if [[ -z "${lsp_dest}" ]]; then
      lsp_dest="${HOME}/.beskid/bin/beskid_lsp"
    fi
    mkdir -p "$(dirname "${cli_dest}")" "$(dirname "${lsp_dest}")"
    install -m 0755 "${cli_built}" "${cli_dest}"
    install -m 0755 "${lsp_built}" "${lsp_dest}"
    echo "Replaced ${cli_dest}"
    echo "Replaced ${lsp_dest}"
