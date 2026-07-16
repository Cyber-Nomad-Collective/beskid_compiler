# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) when applicable.

## [Unreleased]

### Added

- Lower compiler-authorized Corelib syscall services through distinct ABI imports in syntax ISLE
  codegen, without granting ordinary applications or canonical runtime intrinsic authority.
- Add a compiler-minted Corelib syscall service capability and syntax lowering fact that only
  accepts the exact embedded `Core/Syscall/Syscall.bd` corpus.
- Add generation-safe enum-constructor facts and syntax-only ISLE lowering for nullary and
  single-payload variants.
- Add canonical Beskid allocation-header ownership and LIFO root-frame primitives as the
  bounded input to a future non-moving collector.
- Add a manifest-derived ABI-v5 runtime provenance audit and portable explicit-symbol-list verifier.
- Add canonical-runtime `pointer`, `word`, and `never` source signatures plus compiler-minted intrinsic-call authority.
- Categorize every HIR retirement blocker without allowlists and verify all ABI-v5 provenance fixtures from the release gate.
- Inspect Cargo manifests for retired Rust runtime, bridge, and host dependency paths in the ABI-v5 retirement gate.
- Add an explicit native-runtime-kit CI migration diagnostic and fixture test that identifies every remaining runtime-bridge setup caller.
- Add the Apple Silicon native platform-shim artifact for ABI-v5 allocation, release, and trap,
  with manifest-platform import provenance checks and no Rust bridge dependency.
- Add a native-host runtime-kit CLI publisher and CI staging wrapper for exact debug or release
  host artifacts.

### Changed

- Derive direct nominal-local field access and mixed-width integer operands from generation-safe
  syntax facts for ISLE emission, including Corelib `StyleChain` and terminal parsing paths.
- Route Corelib executable-entry lowering from an assembled generation-safe syntax program
  directly through `TypedProgram`, `CodegenInput`, and ISLE, without invoking the legacy HIR
  frontend compatibility path.
- Lower inline nominal struct-literal method calls through generation-safe receiver and ABI facts
  in the syntax-only ISLE path.
- Derive typed-local ABI facts in syntax `test` bodies from the test definition scope, so exact
  generic call specializations remain reachable for ISLE emission.
- Stop syntax ISLE statement cursors after a terminating instruction, preventing unreachable
  trailing source statements from being emitted into a filled CLIF block.
- Run Corelib entry-call gates against generation-safe syntax facts instead of the retired HIR
  semantic resolver, preserving public module re-export authority during the migration.
- Resolve imported type-qualified static calls and inferred generic calls through generation-safe
  syntax facts, including exact instantiated ABI signatures for ISLE emission.
- Resolve syntax-fact module members and nominal types through explicit public re-export edges.
- Keep legacy export metadata fixtures aligned with runtime-handler metadata during the syntax
  migration.
- Use platform-correct local dynamic-loader flags so freshly staged Linux ABI-v5 runtime kits
  can be opened by Engine and external native resolution.
- Separate binary-provenance runtime exports from the ABI-and-assembly symbols required by the JIT loader.

- Name the internal generation-safe LSP syntax-fact result so lifecycle refresh paths preserve
  definitions, hovers, symbols, completion, and inlay hints without positional tuple coupling.
- Audit Linux shared ABI-v5 runtime artifacts with an exact ELF loader-import allowlist while
  preserving the static archive and Rust-runtime linkage boundary.
- Centralize post-mod-rewrite syntax assembly projection at the shared frontend boundary for
  Engine and prepared syntax lowering.
- Serve LSP completion from generation-bound syntax/Salsa facts with exact replacement edits, including imported module members.
- Make the exact installed ABI-v5 runtime kit the sole Engine, JIT, REPL, and in-process test runtime authority.
- Require linked AOT artifacts to use one hash-validated ABI-v5 runtime kit while retaining runtime-free object emission.
- Derive lambda capture environments, spawn operands, and manifest-owned runtime intrinsics from expanded AST/Salsa facts.
- Add a generation-safe expanded-syntax to generated-ISLE adapter for production expression emission.
- Emit zero-parameter parsed function bodies through syntax-only generated ISLE statement rules.
- Route prepared frontend Engine and fixture entrypoints through generation-safe syntax,
  `TypedProgram`, `CodegenInput`, and ISLE module emission rather than typed HIR.
- Lower compiler-authorized canonical runtime intrinsic calls through syntax-only ISLE module
  emission, with manifest-derived imports and verified CLIF coverage for allocation and root-frame
  helpers.
- Derive contextual primitive cast intents from direct and canonical ABI-v5 intrinsic call
  parameters so runtime `word` offsets are typed before ISLE emission.

### Fixed

- Preserve complete logical module names for assembled `.generated/*.g.bd` units so syntax
  consumers do not register `Generated` modules under a truncated name.
- Terminate reachable syntax-ISLE control-flow merge blocks after one-arm `if` statements and
  preserve valid unreachable merges when both arms return.

### Removed

- Remove the obsolete Rust language-handler regeneration hook, which targeted the retired
  runtime.v1 manifest rather than the canonical ABI-v5 contract.
- Remove the borrowed `FrontEndLowerInput` / HIR-only entrypoint codegen boundary.

- Remove AOT prebuilt-archive and standalone fallbacks, runtime link profiles, and host-archive lookup.

- Remove legacy Rust runtime registration, Engine-owned Rust runtime state, scheduler/TLS wrappers, and JIT `std`/`minimal` profile selection.
