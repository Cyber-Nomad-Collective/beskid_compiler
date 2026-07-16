# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) when applicable.

## [Unreleased]

### Added

- Add a manifest-derived ABI-v5 runtime provenance audit and portable explicit-symbol-list verifier.
- Add canonical-runtime `pointer`, `word`, and `never` source signatures plus compiler-minted intrinsic-call authority.
- Categorize every HIR retirement blocker without allowlists and verify all ABI-v5 provenance fixtures from the release gate.
- Add an explicit native-runtime-kit CI migration diagnostic and fixture test that identifies every remaining runtime-bridge setup caller.

### Changed

- Serve LSP completion from generation-bound syntax/Salsa facts with exact replacement edits, including imported module members.
- Make the exact installed ABI-v5 runtime kit the sole Engine, JIT, REPL, and in-process test runtime authority.
- Require linked AOT artifacts to use one hash-validated ABI-v5 runtime kit while retaining runtime-free object emission.
- Derive lambda capture environments, spawn operands, and manifest-owned runtime intrinsics from expanded AST/Salsa facts.
- Add a generation-safe expanded-syntax to generated-ISLE adapter for production expression emission.
- Emit zero-parameter parsed function bodies through syntax-only generated ISLE statement rules.
- Route prepared frontend Engine and fixture entrypoints through generation-safe syntax,
  `TypedProgram`, `CodegenInput`, and ISLE module emission rather than typed HIR.

### Removed

- Remove the borrowed `FrontEndLowerInput` / HIR-only entrypoint codegen boundary.

- Remove AOT prebuilt-archive and standalone fallbacks, runtime link profiles, and host-archive lookup.

- Remove legacy Rust runtime registration, Engine-owned Rust runtime state, scheduler/TLS wrappers, and JIT `std`/`minimal` profile selection.
