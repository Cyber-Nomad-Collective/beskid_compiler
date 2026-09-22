# corelib_glue

Beskid.Glue: the contract layer for multi-language integration.

This package declares the seven atomized glue contracts, the glue attributes (`[Glue]`, `[GlueImport]`, `[GlueExport]`), the host typed tag object (`GlueTag`), and the stdio bridge fiber types.

## Contracts (0.4 cutoff)

- `TypeMapping` — map Beskid surface types to `Interop.Contracts` type-shapes
- `SymbolEmission` — emit foreign symbols
- `LinkArgs` — resolve linker arguments
- `SignatureReader` — read foreign signatures (dotnet via dotscope in 0.5)
- `SignatureWriter` — write foreign signatures
- `ToolchainProbe` — discover and validate external tools (fail-closed)
- `StdioBridge` — generate the stdio message-bridge fiber

## Attributes

- `[Glue(backend: "...")]` — marks a type or function as a glue entry point
- `[GlueImport(library: "...")]` — marks a contract or function as an imported foreign binding
- `[GlueExport(library: "...")]` — marks a function as exported to a foreign library

## 0.5 scope

Language-specific code generation (Rust emitter, .NET emitter via dotscope), full `ToolchainProbe` implementation, stdio protocol implementation, and corelib/runtime glue integration.
