# Compiler workspace — common test tasks.
#
#   just corelib    Run corelib_tests via release beskid_cli
#   just compiler   Run cargo test for the workspace
#   just tests      Run compiler and corelib tests
#   just replace    Atomically install one exact local CLI/LSP/runtime/corelib/package bundle
#   just vscode     Build and reinstall the VS Code/Cursor extension from `beskid_vscode`

set shell := ["bash", "-euo", "pipefail", "-c"]

root := justfile_directory()

vscode_dir := root + "/../beskid_vscode"

corelib_tests_project := "corelib/beskid_corelib/tests/corelib_tests"

default:
    @just --list

# Run every corelib_tests target via the installed beskid_cli (shared Salsa session).
corelib:
    just replace
    "$HOME/.beskid/bin/beskid" test \
        --project "{{corelib_tests_project}}" \
        --all-targets \
        </dev/null

# Run the full compiler workspace test suite.
compiler:
    cargo test

# Run compiler and corelib tests.
tests: compiler corelib

# Build one exact-version release bundle and replace the installed toolchain.
replace:
    #!/usr/bin/env bash
    set -euo pipefail
    compiler_dir="{{root}}"
    super_root="$(cd "${compiler_dir}/.." && pwd)"
    required_superrepo_paths=(
      "scripts/ci/resolve-editor-authoring-version.mjs"
      "scripts/ci/build-release-artifact.sh"
      "beskid_distrib/scripts/extract-release-bundle.sh"
      "beskid_vscode/package.json"
    )
    missing_superrepo_paths=()
    for relative_path in "${required_superrepo_paths[@]}"; do
      [[ -e "${super_root}/${relative_path}" ]] || missing_superrepo_paths+=("${relative_path}")
    done
    if [[ "${#missing_superrepo_paths[@]}" -ne 0 ]]; then
      echo "just replace requires the aggregate beskid superrepo checkout." >&2
      printf 'Missing required sibling path: %s\n' "${missing_superrepo_paths[@]}" >&2
      echo "Run ./scripts/setup-environment.sh from the beskid superrepo root, then retry." >&2
      exit 1
    fi
    release_version="$(node "${super_root}/scripts/ci/resolve-editor-authoring-version.mjs" "${super_root}" "${super_root}/beskid_vscode")"
    rust_target="$(rustc -vV | awk '/^host: / { print $2 }')"
    asset_name=".local-beskid-bundle-${release_version}-$$.tar.gz"
    artifact="${super_root}/${asset_name}"
    staging="$(mktemp -d)"
    cleanup() {
      rm -f "${artifact}"
      rm -rf "${staging}"
    }
    trap cleanup EXIT
    bash "${super_root}/scripts/ci/build-release-artifact.sh" \
      beskid_bundle ignored "${rust_target}" "${asset_name}" "${release_version}"
    bash "${super_root}/beskid_distrib/scripts/extract-release-bundle.sh" \
      "${artifact}" "${release_version}" "${rust_target}" "${staging}/bundle"
    install_prefix="${HOME}/.beskid"
    bash "${compiler_dir}/scripts/install-local-toolchain-bundle.sh" \
      "${staging}/bundle" "${release_version}" "${install_prefix}"

# Worktree tip: to share one target dir across git worktrees and avoid rebuilding per worktree,
# run: CARGO_TARGET_DIR=$PWD/target cargo check   (or export it in your shell)
# sccache (wired in .cargo/config.toml) covers the cache either way.

# Fast type-check the default member set (excludes the test sink crates).
check:
    cargo check --workspace \
        --exclude beskid_e2e_tests \
        --exclude beskid_tests_surface \
        --exclude beskid_tests_projects \
        --exclude beskid_tests_mods \
        --exclude beskid_tests_lsp \
        --exclude beskid_tests_aot \
        --exclude beskid_tests_pckg \
        --exclude beskid_tests_interop \
        --exclude beskid_tests_cli \
        --exclude beskid_tests_composition \
        --exclude beskid_tests_abi \
        --exclude beskid_tests_support

# Type-check one crate + its direct deps only.
check-p crate:
    cargo check -p {{crate}}

# Remove build artifacts unused for 30 days (requires `cargo install cargo-sweep`).
clean-stale:
    @command -v cargo-sweep >/dev/null 2>&1 || { echo "Install: cargo install cargo-sweep"; exit 1; }
    cargo sweep -t 30

# Build beskid_vscode and reinstall into Cursor or VS Code (reload window after).
vscode:
    #!/usr/bin/env bash
    set -euo pipefail
    vscode_dir="{{vscode_dir}}"
    compiler_dir="{{root}}"
    super_root="$(cd "${compiler_dir}/.." && pwd)"
    if [[ ! -f "${vscode_dir}/package.json" ]]; then
      echo "beskid_vscode not found at ${vscode_dir} — run ./scripts/setup-environment.sh" >&2
      exit 1
    fi
    release_version="$(node "${super_root}/scripts/ci/resolve-editor-authoring-version.mjs" "${super_root}" "${vscode_dir}")"
    rust_target="$(rustc -vV | awk '/^host: / { print $2 }')"
    platform_key="$(node -p 'process.platform + "-" + process.arch')"
    binary_name="beskid_lsp"
    if [[ "${rust_target}" == *-windows-* ]]; then
      binary_name="beskid_lsp.exe"
    fi
    asset_name=".local-beskid-lsp-${release_version}-$$"
    artifact="${super_root}/${asset_name}"
    cleanup() {
      rm -f "${artifact}"
    }
    trap cleanup EXIT
    bash "${super_root}/scripts/ci/build-release-artifact.sh" \
      beskid_lsp beskid_lsp "${rust_target}" "${asset_name}" "${release_version}"
    server_dir="${vscode_dir}/server/${platform_key}"
    mkdir -p "${server_dir}"
    install -m 0755 "${artifact}" "${server_dir}/${binary_name}"
    [[ "$("${server_dir}/${binary_name}" --version 2>&1)" == "beskid_lsp ${release_version}" ]]
    cd "${vscode_dir}"
    bun install
    bun run build
    mkdir -p dist
    BESKID_VSCODE_SKIP_PREBUILD=1 bunx @vscode/vsce package --out dist/beskid-dev.vsix
    vsix="${vscode_dir}/dist/beskid-dev.vsix"
    installed=0
    if command -v cursor >/dev/null 2>&1; then
      cursor --install-extension "${vsix}" --force
      echo "Reinstalled Beskid extension in Cursor — Developer: Reload Window"
      installed=1
    fi
    code_cli="$(command -v code 2>/dev/null || true)"
    if [[ -z "${code_cli}" && -x "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code" ]]; then
      code_cli="/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"
    fi
    if [[ -n "${code_cli}" ]]; then
      "${code_cli}" --install-extension "${vsix}" --force
      echo "Reinstalled Beskid extension in VS Code — Developer: Reload Window"
      installed=1
    fi
    if [[ "${installed}" -eq 0 ]]; then
      echo "Packaged ${vsix} but no cursor/code CLI on PATH" >&2
      exit 1
    fi
