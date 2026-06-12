# Compiler workspace — common test tasks.
#
#   just corelib    Run corelib_tests via release beskid_cli
#   just compiler   Run cargo test for the workspace
#   just tests      Run compiler and corelib tests
#   just replace    Build release CLI + LSP and overwrite installed `beskid` / `beskid_lsp`
#   just vscode     Build and reinstall the VS Code/Cursor extension from `beskid_vscode`

set shell := ["bash", "-euo", "pipefail", "-c"]

root := justfile_directory()

vscode_dir := root + "/../beskid_vscode"

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

# Build beskid_vscode and reinstall into Cursor or VS Code (reload window after).
vscode:
    #!/usr/bin/env bash
    set -euo pipefail
    vscode_dir="{{vscode_dir}}"
    if [[ ! -f "${vscode_dir}/package.json" ]]; then
      echo "beskid_vscode not found at ${vscode_dir} — run ./scripts/setup-environment.sh" >&2
      exit 1
    fi
    cd "${vscode_dir}"
    bun install
    bun run build
    mkdir -p dist
    BESKID_VSCODE_SKIP_PREBUILD=1 bunx @vscode/vsce package --out dist/beskid-dev.vsix
    vsix="${vscode_dir}/dist/beskid-dev.vsix"
    if command -v cursor >/dev/null 2>&1; then
      cursor --install-extension "${vsix}" --force
      echo "Reinstalled Beskid extension in Cursor — Developer: Reload Window"
    elif command -v code >/dev/null 2>&1; then
      code --install-extension "${vsix}" --force
      echo "Reinstalled Beskid extension in VS Code — Developer: Reload Window"
    else
      echo "Packaged ${vsix} but no cursor/code CLI on PATH" >&2
      exit 1
    fi
