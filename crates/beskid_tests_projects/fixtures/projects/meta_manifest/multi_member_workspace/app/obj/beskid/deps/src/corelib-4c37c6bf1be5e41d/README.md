# Beskid corelib (`corelib`)

Standard library sources for the Beskid toolchain. Registry package identity: **`corelib`**.

## Documentation

Module contracts, runtime boundaries, and design notes live under **[`docs/`](./docs/README.md)**. These Markdown files are included in **`beskid pckg pack`** artifacts at `docs/**/*.md`. When you run pack on this project, the CLI also emits API listings under **`.beskid/docs/`** (`api.json`, `index.md`) for the registry documentation viewer.

## Layout

- **`src/`** — Beskid implementation (`Prelude.bd` is the library root for the `CoreLib` target).
- **`tests/corelib_tests/`** — Beskid `test` items compiled as separate targets for CI smoke checks (`project.name = corelib_tests`, path dependency on aggregate `corelib`). Remove stale artifacts with `rm -rf tests/corelib_tests/obj` before local runs if dependency layout changes (committed `obj/` is gitignored).

Publishing and CI entry points are described in [`../README.md`](../README.md) from this nested directory (compiler repository).
