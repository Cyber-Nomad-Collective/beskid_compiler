# Beskid Compiler

Beskid exists because I’m done with C#/.NET’s abstraction hell and runtime identity crisis.
This compiler is where language features are *language features* — not ten layers of corelib and DI containers.

The entire point of this project is to be more precise with hating microsoft for the lack of direction in csharp.

### IoC? Yes. IoC frameworks? No.

IoC is a good direction, but it belongs in the **compiler**, not in a forest of framework glue.

- No DI container black boxes.
- No “inject everything because we can.”
- If IoC exists, it should be **explicit, verifiable, and compiled**, not hidden behind runtime indirection.

### The pain points we’re cutting out

- **Corelib abstraction sprawl.** When core functionality becomes a maze of wrappers, you’re not writing software, you’re negotiating with an API.
- **Reflection as a crutch.** Slow, opaque, runtime‑only power instead of compile‑time truth.
- **CIL stagnation.** Language features outpace the VM, so everything devolves into workarounds and performance tax.

### What Beskid pushes forward

- **Enumerators and iteration as first‑class language features.**
- **Metaprogramming over reflection.** Compile‑time power, real guarantees.
- **Assembly output without IL handcuffs.**

### Non‑Goals

- Being “enterprise-friendly.”
- Competing with a colossal framework stack.
- Hiding complexity behind runtime containers.

### Status

Opinionated compiler project. Not finished. Not apologizing.

The latest announced release is 0.5.3. 0.6.0 and 0.6.6 are in qualification and not released; see the 0.6.6
and 0.6.0 sections of [CHANGELOG.md](CHANGELOG.md).

### How the compiler is built (0.6)

- **One front end.** The `beskid_queries` query gate is the only semantic authority for `check`,
  `build`, `run`, `test` and the language server. The executable front end comes from the same gate,
  and codegen lowers it through `TypedProgram`, `CodegenInput` and ISLE to stock Cranelift CLIF
  (`docs/design/2026-10-12-query-front-end.md`). There is no second type checker and no HIR.
- **Mods.** Compiler Mods are Beskid packages compiled ahead of time and run in the native Mod host.
  Each artifact has a `mod.descriptor.json` (schema 3) that records its sources, SDK, runtime binding
  and build tools by digest. The toolchain Corelib bundle ships its Mods prebuilt; other Mods are
  built by `beskid dev mod rebuild` or by any `build`, `run` or `check` of the consuming project.
- **Rust Glue.** `beskid build --backend glue-rust` builds a `Lib` target as a shared Glue consumer
  plus one Rust owner image for each manifest `glue "<library>"` block. It needs `--linker` and either
  `--rust-toolchain` or `--cargo` with `--rustc`. Images carry admission records that pin each other by
  digest. `glue-dotnet` is unavailable.
- **Runtime limits.** Runtime tables grow on demand. `BESKID_HEAP_MAX` caps the managed heap (no cap
  by default) and `BESKID_FIBER_STACK_MAX` sets the per-fiber stack reservation (default 8 MiB). The
  runtime reads both at process init.
- **Concurrency.** Fibers are the concurrency model. Corelib has no raw OS-thread API;
  `Core.Threading.Thread.Yield` is the only thread-level call.

### Commands

```text
beskid new <name>            create a project from the bundled template
beskid check | build | run | test | fmt | doc
beskid add <package>[@<version>] | remove <package> | update <package> | update --all
beskid package ...           search, publish and manage packages and templates
beskid toolchain status | update
beskid doctor                diagnose the installation without changing it
beskid dev ...               syntax, build, project, corelib, runtime-kit, mod, import, lsp, bsol, repl
```

A command that moved (for example `beskid analyze` or `beskid lsp`) fails with the name of its
replacement (`beskid check`, `beskid dev lsp`).

### Formatter / pretty-print

The analysis crate includes an opinionated source formatter based on an `Emit` trait. Use
`beskid_analysis::format::format_program` on a successfully parsed `Program`, or `beskid fmt` on files.

### Prebuilt releases

Woodpecker builds and checks the native targets; publication to GitHub releases is a separate,
reviewed manual step. Each release has immutable `cli-vX.Y.Z` and `lsp-vX.Y.Z` tags and moves the
rolling tags of its channel:

| Stream | Rolling tags | Version file | Assets |
|--------|--------------|--------------|--------|
| CLI | `cli-stable`, `cli-unstable` | `cli-version.txt` | `beskid-linux-amd64`, `beskid-darwin-arm64`, `beskid-windows-amd64.exe` |
| LSP | `lsp-stable`, `lsp-unstable` | `lsp-version.txt` | `beskid_lsp-linux-amd64`, `beskid_lsp-darwin-arm64`, `beskid_lsp-windows-amd64.exe` |

Install the managed LSP with `beskid dev lsp install` (default tag `lsp-stable`; it writes
`~/.beskid/bin/beskid_lsp`). `beskid dev lsp` prefers that binary when present.

### Development setup

Install Rust through [rustup](https://rustup.rs). The `rust-toolchain.toml` in this directory (channel
`1.98.1`, matching the Woodpecker image) is only honored through rustup's shims, so keep `~/.cargo/bin` ahead of other Rust installs.
A Homebrew `rustc` first on `PATH` fails with "linker `rust-lld` not found", because
`.cargo/config.toml` selects `rust-lld` on `aarch64-apple-darwin`. The workspace needs Rust 1.96 or newer.
Initialize the corelib submodule with `git submodule update --init corelib` before building.

### Building

The **core library sources** are pinned as a Git submodule at `corelib/` (repository: [beskid_standard](https://github.com/Cyber-Nomad-Collective/beskid_standard)), under `corelib/beskid_corelib/` (or `compiler/corelib/beskid_corelib/` from the aggregate superrepo root). The CLI and the language server embed that tree at build time (`crates/beskid_corelib_embed/build.rs`).

From an aggregate `beskid` checkout, `just replace` builds the exact shared editor release version and
installs its CLI, LSP, updater, ABI-v5 runtime kit, corelib, and bundled packages under `~/.beskid`.
The complete bundle is validated in a same-filesystem staging directory before the existing prefix is
replaced, so files removed by a release do not survive and a failed publication restores the prior
installation. Non-toolchain entries such as `~/.beskid/data` and `~/.beskid/config` are carried into
the staged prefix unchanged. This recipe intentionally requires the superrepo's `scripts`, `beskid_distrib`,
`beskid_vscode`, and initialized submodules; a standalone compiler checkout exits with the missing
prerequisite paths instead of starting a partial build.

## Licensing

The compiler, CLI, LSP, reusable libraries, canonical runtime, and core library
are Apache-2.0. The runnable `beskid_pckg_server` package is the scoped
AGPL-3.0-only exception. Compiling a program does not impose either license on
user-authored code; linked programs include Apache-2.0 runtime and possibly
core-library portions whose license and notices must be preserved when those
portions are distributed. See [LICENSING.md](LICENSING.md) for the exact scope.

Corelib package publishing authority lives in `beskid_standard` CI; compiler CI validates and consumes corelib but does not publish it.

Clone with submodules (required for the embedded corelib):

```bash
git clone --recurse-submodules https://github.com/Cyber-Nomad-Collective/beskid_compiler.git
# or, after a clone without submodules:
git submodule update --init --recursive
```

In an aggregate repo that vendors this tree as a submodule (for example the [`beskid` superrepo](https://github.com/Cyber-Nomad-Collective/beskid)), initialize **recursively** so `compiler/corelib` is populated: `git submodule update --init --recursive` (or clone with `--recurse-submodules`). Override source root with `BESKID_CORELIB_SOURCE` if needed.
