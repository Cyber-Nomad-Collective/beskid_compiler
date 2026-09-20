This directory is the canonical home for **corelib** human-facing documentation: module contracts, boundaries, and API notes. It ships inside **`beskid pckg pack`** artifacts under the `docs/` prefix (alongside generated `.beskid/docs/` API listings when the CLI runs doc generation during pack).

The public docs site under `site/website/src/content/docs/corelib/` holds short pointers to these files so the Beskid sources remain the editorial source of truth.

## Scope

- Public module naming and surface aligned with `compiler/corelib/beskid_corelib/src`.
- Runtime boundaries (`Core.Syscall`, `Core.Input` / `Core.Output` / `Core.Error`, `Console`) versus staged placeholders (`Core.FS`, `Core.Environment`, …).
- Error model (`Core.Results`, structured errors on system modules).

These pages are **not** platform-spec nodes: cross-cutting policy fields (`specLevel`, `owner`, …) belong under `site/website/src/content/docs/platform-spec/` and are validated separately by trudoc.

## Index

- [Core](./Core/README.md): [Error-Handling](./Core/Error-Handling.md), [Results](./Core/Results.md), [String](./Core/String.md)
- [Collections](./Collections/README.md): [Array](./Collections/Array.md), [List](./Collections/List.md), [Map](./Collections/Map.md), [Set](./Collections/Set.md), [Queue](./Collections/Queue.md), [Stack](./Collections/Stack.md)
- [Query](./Query/README.md): [Operators](./Query/Operators.md), [Execution](./Query/Execution.md)
- [Core](./Core/README.md) OS runtime: [Syscall](./Core/Syscall.md), [Input](./Core/Input.md), [Output](./Core/Output.md), [Error](./Core/Error.md), [FS](./Core/FS.md), [Path](./Core/Path.md), [Time](./Core/Time.md), [Environment](./Core/Environment.md), [Process](./Core/Process.md)
- [Testing](./Testing/README.md): [Contracts](./Testing/Contracts.md), [Assert](./Testing/Assert.md)

## Naming direction

- Do not use a `Std` prefix in public API examples.
- Use canonical module names (`IO`, `String`, `Array`, `Query`, …).
- Follow C#-style naming for public APIs (`PascalCase` types and functions).

## Relationship to platform spec

Cross-cutting policy for naming and boundaries appears under `/platform-spec/`. This directory holds **per-module** contracts tied to the checked-in sources.

## Source and release policy

- Canonical implementation stays in the compiler **`corelib`** submodule (`beskid_corelib`).
- Registry package id is **`corelib`**.
- Release versioning follows the shared compiler/language release train.

## Legacy notes

Older numbered scaffolding (`00-*` … `07-*`) is superseded by this folder layout.
