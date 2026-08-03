# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) when applicable.

## [Unreleased]

### Removed

- Retire the unused `beskid_runtime_bridge` static archive package and its
  source-tree build/CI compatibility helpers; the retirement gate now rejects
  restoring the bridge package or workspace member.

### Changed

- Migrate AOT and parsed-project codegen tests to the production prepared-syntax
  → `CodegenInput` → ISLE boundary, removing test reliance on the retired HIR
  `lower_program` compatibility path.
- Prepare-spine frontend assembly projects `SyntaxProgramAssembly` as the IDE/query
  authority (`PreparedCompilation::syntax_assembly`); LSP lifecycle binds facts from
  `FrontEndTypedResult::syntax_assembly` (post-mod-rewrite entry), and
  `beskid_queries::syntax_program_assembly` strips HIR at the Salsa assembly boundary.
  `DocumentAnalysisSnapshot` remains CLI-doc-only (CYB-65).

### Fixed

- Allocate enum constructor values through ABI-v5 managed-object metadata rather
  than returning pointers to constructor stack storage, preventing invalid enum
  tags and JIT traps across function boundaries.


- Replace canonical composition sentinel exports with per-runtime, container-owned
  frozen-plan storage. Registration and plural binding order are now validated
  before activation; resolution is closed over that plan and foreign, stale,
  null, or post-activation mutation attempts fail closed.
- Replace canonical event runtime-state table addressing with lazily allocated,
  field-slot-owned subscription state. Resolved capacities, ordered lookup, and
  stable first-match removal now share the canonical Beskid ABI surface.
- Move canonical PubSub Hub storage out of undeclared `RuntimeState` offset
  arithmetic into a scheduler-owned allocation. Hub registrations now support
  256 stable entries, replace an existing index in place, preserve ordering on
  removal, and keep the circular receive cursor valid.

- Replace the canonical callback literal runtime-state table with a
  manifest-declared, per-runtime registry. Callback and handler registration
  now validate before publishing a complete replacement snapshot, resolve
  duplicates deterministically, and reject unregistered trampoline targets.
- Preserve canonical runtime module-constant layout values at their declared
  direct-call ABI width under compiler-minted authority, while rejecting literal
  coercion and untrusted source. Runtime lowering now reports the first failed
  nested AST statement or expression instead of collapsing errors to its block.
- Derive scheduler fiber-context allocation metadata and context entrypoints
  exclusively from the selected ABI-v5 target manifest, including exact
  assembly-export validation and generated ABI contract updates.
- Add manifest-owned guarded scheduler stacks with a no-access lower guard and
  writable usable suffix across Linux, Darwin, and Windows adapters; canonical
  intrinsic arguments now materialize layout-constant paths at their exact ABI
  parameter width, including nested pointer arithmetic.

- Move canonical WaitGroup state out of undeclared `RuntimeState` offset
  arithmetic into a separately allocated scheduler-owned object. Its waiter
  registry now matches the current sixteen-fiber scheduler bound and queues
  every registered waiter once when `Done` reaches zero; the remaining
  cooperative parking/context-switch execution work is intentionally separate.
- Move canonical mutex table storage out of undeclared `RuntimeState` offset
  arithmetic into a separately allocated, zero-initialized scheduler-owned
  object, with source-level ownership and non-aliasing regressions.
- Move canonical channel table storage out of undeclared `RuntimeState` offset
  arithmetic into a separately allocated, zero-initialized scheduler-owned
  object, with source-level ownership and non-aliasing regressions.
- Remove the duplicate canonical `gc_external_root_count` C-ABI export that
  incorrectly returned handle-table occupancy; the sole export now reports the
  GC root registry and is covered from empty through register/unregister.
- Align the guarded-match formatter golden with canonical nullary enum
  constructor syntax, which omits redundant empty parentheses.
- Restore formatter fixture classification for top-level `const` declarations,
  with a parse-format-reparse regression test for canonical constant emission.
- Parse canonical runtime bitwise shifts and OR composition through the syntax-fact → ISLE path,
  emit stock CLIF `ishl`/`ushr`/`bor`, and treat `parent` and `event` as contextual structural
  terms so ABI-v5 runtime pointer parameters remain valid identifiers.
- Map syntax-indexed constants through the explicit LSP document-symbol and semantic-token
  representations, preserving exhaustive syntax-fact presentation after `AnalysisSymbolKind`
  gained `Constant`.
- Authorize only the compiler-embedded Foundation `Core.Error` source to import
  `__panic_str`, preserving the existing fail-closed Dynamic path for untrusted or unresolved
  calls while allowing its canonical enum-match error arm to derive the exact panic ABI
  (CYB-160).
- Make the native ABI-v5 runtime-kit matrix script enter the compiler workspace before
  running Cargo, so the Windows superproject workflow no longer fails before kit staging
  with a missing root-level `Cargo.toml` (CYB-112 follow-up).
- Allocate aggregate literals through the canonical ABI-v5 managed-object request path with
  header-aware field offsets and emitted descriptor/pointer-map metadata, preventing escaped
  stack storage from producing null Corelib string handles (CYB-158/CYB-159).
- Make the native ABI-v5 runtime-kit matrix script enter the compiler workspace before
  running Cargo, so the Windows superproject workflow no longer fails before kit staging
  with a missing root-level `Cargo.toml` (CYB-112 follow-up).
- Register the Corelib syscall read/write process symbols with the exact-kit JIT builder, so
  `Core.Output.WriteLine` links without treating process symbols as ABI-v5 runtime-kit exports.
- Exclude event declarations from ABI-v5 aggregate value layouts, preserving exact value-field
  projection facts for event-bearing nominal controls while event projections remain unavailable
  (CYB-162).
- Preserve the mutable-local syntax fact for canonical `Ansi.Escape.PrivateMode`
  string reassignment, while immutable local assignments remain unavailable (CYB-163).
- Evaluate direct-call lowering for control flow without retaining a unit-valued binding, so the
  compiler's warnings-denied Clippy gate proceeds while preserving the existing call type result.
- Declare the `Core.Syscall` and `Core.Results` module imports used by canonical
  `Core.Error`, preserving direct semantic facts for its qualified `WriteWith`
  call without broadening module lookup (CYB-161).
- Add the manifest-derived `beskid_rt_v5_managed_object_allocate` export and route
  closure environments through its single fail-closed descriptor, size, alignment,
  zeroing, and object-header initialization path (CYB-157).
- Resolve nested qualified generic type arguments before their enclosing type paths,
  materialize concrete generic enum layouts from constructor use sites, and route
  syntax-lowered string literals through the canonical ABI-v5 `str_new` dispatch.
  This restores fail-closed type/layout facts and prevents JIT string operations from
  interpreting literal payload bytes as `BeskidStr` headers (CYB-135/CYB-136/CYB-134).
- Preserve exact generic parameter specializations in syntax-ISLE equality so string
  equality and inequality dispatch through `str_eq`, while pointer-shaped nominal
  values retain identity comparison. Resolve the ABI of unique, public, fully-qualified
  generic nominal return envelopes from their assembled module without weakening the
  general resolver (CYB-138/CYB-139).
- Derive scalar and nominal ABI facts for direct fields on explicitly annotated nominal
  locals and parameters, so generic assertions can specialize `ProgressBar<T>.percent`
  without reopening inferred or chained member typing (CYB-140).
- Declare the `Core.Results` dependency in Foundation `Core.Output`, preserving the
  import provenance required to derive its qualified generic `Result` match layout
  without broadening module discovery (CYB-140).
- Derive a qualified imported payload-enum layout only from one assembled source
  module and one public exported nominal type, allowing `Descriptor::Standard` to
  retain syntax-ISLE facts while unresolved or ambiguous paths remain unavailable
  (CYB-144).
- Infer a genericless enum constructor application only from an immediate explicit
  typed `let` or function/method return context; nested or uncontextualized
  constructors remain unavailable, allowing canonical `Result::Error` facades to
  retain their concrete layout facts without weakening type resolution (CYB-148).
- Materialize source-proven nominal function parameters from their existing ABI
  signature as target pointers, while unspecialized generic parameters remain
  unavailable to local lowering (CYB-151).
- Lower exactly one direct identifier payload in a supported enum-match arm as a
  scoped local, preserving ABI shape for nominal pointer payloads; literal, nested,
  and guarded patterns remain unavailable (CYB-150).
- Derive the exact scalar or nominal ABI type of a direct enum-pattern binding from
  its enclosing enum-match fact, enabling nested nominal pattern scrutinees without
  reopening general local-type inference (CYB-153).
- Compose a supported enum-match result only from its source-proven arm-body node
  facts, so direct pattern bindings participate in nested result typing while mixed
  arm types remain unavailable (CYB-154).
- Derive a direct call expression type only from its exact resolved call ABI result,
  allowing enum-match arms that call typed Syscall helpers while dynamic and unresolved
  calls remain unavailable (CYB-155).
- Authorize the exact canonical Foundation `Core.Output` corpus to import the
  `__panic_str` Corelib service, while keeping byte-identical user copies outside the
  trusted capability boundary (CYB-141).
- Preserve the concrete applied enum layout for explicitly typed local and parameter
  matches, and lower discard-only unit match arms in statement context without
  introducing a scalar merge value (CYB-137).
- Lower direct zero-return calls used as generic enum match-arm statements through the
  existing direct-call ABI path, rather than rejecting them as missing statement facts
  (CYB-137).
- Normalize the Syscall ergonomics fixture to the nullary `StandardStream::Stdout`
  constructor form and remove its unused module import (CYB-132).
- Migrate the `ansi_csi_bold_red` spine test off retired HIR codegen and onto the
  syntax lowering path (`lower_corelib_tests_entrypoint`), then validate the
  resulting artifact directly so this test can no longer trigger retired HIR facade
  failures.
- Add a slow-path CLIF dump test for `ansi_csi_bold_red` to preserve regression
  visibility into argument-order and call-shape lowering around ANSI escape
  sequence calls.
- Enable the main-thread runtime-root bootstrap before executing a JIT entrypoint in
  `run_syntax_jitted_entrypoint`. In-process JIT execution (`beskid test`, matrix test, REPL)
  has no `beskid_runtime_link_anchor`, so allocating entrypoints (string interpolation, gc
  roots) aborted with `no active runtime root`; the runtime now lazily installs a default
  heap/root exactly as AOT-linked executables do.
- Accept the `Ok(None)` (no specialization) outcome for unavailable call sites in the
  `imported_generic_nominal_calls_require_receiver_instantiation` semantic-facts test, matching
  the `generic_call_specialization` contract that no longer propagates unavailable errors.
- Register process-linked soft-builtin addresses (`interop_dispatch_*`, `panic_str`, …) on
  exact ABI-v5 JIT modules so Cranelift can resolve imports after validation allowlists them.
- Give string `+` / `==` / `!=` BinaryExpression nodes STRING/BOOL `abi_type` and `node_type`
  facts so interpolated-string desugars lower through ISLE `StringAdd` instead of failing coerce.
- Skip unavailable `call_lowering` inside `generic_call_specialization` (no specialization) so
  reachable Syscall/Output bodies with unresolved enum-constructor calls do not abort module emission.
- Forward the Windows COFF import library from `emit_host_platform_library_pair` through
  `build_native_host` and fail closed when a COFF host kit omits it (CYB-112).
- Lexically normalize compiler-owned Corelib service source paths and match Foundation
  `source_root` without symlink canonicalize, so materialized `Testing/Assert.bd` retains
  `__panic_str` CorelibService provenance while user symlinks stay Dynamic.
- Allow process-linked soft builtins (`interop_dispatch_*`, `panic_str`, …) in exact ABI-v5
  JIT symbol validation, matching AOT `linking::validate` so Corelib tests that clear ISLE
  are not rejected solely for kit export-allowlist absence.

### Removed

- Remove LSP `ANALYSIS_CACHE_VERSION` / `Document.analysis_cache_version`. Hard invalidation
  clears generation-bound syntax facts (fail closed) and rebuild paths rebind documentation /
  diagnostics without a shape-version cache (CYB-78).

### Added

- Add generation-bound `for_iterator_fact` for `ForStatement` range loops (iterator declaration
  identity + element type), wire ISLE `scalar_type`/`local_slot` through that fact, and prove
  shadowing plus stale-generation fail-closed (CYB-92).
- Expand generation-safe `completion_candidates` with lexical locals, type/enum names, and
  explicitly annotated nominal receiver fields/methods (CYB-19). Inferred receivers stay
  unavailable.
- Add the OpenSpec 6.8.1a trap scenario regression: canonical `Trap` maps ABI `usize` to
  source `word`/`never`, retains a source span, and ordinary packages cannot import
  `beskid_rt_v5_trap` (CYB-18).
- Expand the parsed-project production harness with inline method reachability, generation-safe
  capture-fact proof plus fail-closed capturing lambda lowering, and canonical-runtime trusted
  intrinsic lowering through `lower_canonical_runtime_prepared_syntax` (CYB-12/CYB-15).
- Include `MethodDefinition` callees in generation-safe `direct_callees` / `reachable_items` so
  production `lower_syntax_assembly_entrypoint` retains inline methods already proven by
  `call_lowering` (CYB-15/CYB-64).
- Migrate the LSP intellisense member-completion fixture off legacy
  `beskid_analysis::services::completion_candidates` onto syntax-only `handle_completion` with no
  document analysis snapshot (CYB-19).
- Span-bearing `MissingRuleOrFact` rejection fixtures for remaining unsupported inventory
  families (host composition, try) and inventory audit rows flipped from `CodexBlocker` to
  `Present` (CYB-106 under CYB-81).
- Complete CYB-81 inventory acceptance: pin explicit 0.4 release-rejection rationales for every
  `UnsupportedTypedOperation` kind, and port bounds-checked array index assignment through
  generated ISLE (`emit_index_assign`) with a verified CLIF regression (CYB-81).
- Add the manifest-derived ABI-v5 `beskid_rt_v5_fiber_spawn_with_cancel_slot` scheduler
  boundary and direct syntax-ISLE import metadata, replacing syntax-spawn emission of the
  retired `interop_dispatch_*` envelope path (CYB-126). The end-to-end macOS arm64 JIT smoke
  remains tracked separately as CYB-129 after a direct JIT-to-runtime dylib call raised SIGILL.

### Fixed

- Exact native kit provenance now requires `beskid_rt_v5_closure_environment_root_current`
  beside allocate/capture-store/root, and the JIT fail-closed closure descriptor/rooting
  regression runs on Darwin arm64 as well as Linux x86_64 with an explicit null-request
  allocate guard (CYB-131 under CYB-79/CYB-109).
- Purge LSP intellisense tests off `DocumentAnalysisSnapshot` fallbacks; definition/references
  corelib fixtures now require syntax facts via `build_document` (CYB-26/CYB-27 residual).
- Audit native-host static runtime kits with `verify_static_archive` so Linux GD TLS `__tls_get_addr` is expected, not a provenance failure.
- Update the canonical-runtime source contract to require wrapping overflow guards in
  `ValidatePointerMap` instead of signed `NativeWordMax() - 8` compares (CYB-129).
- Replace `ValidatePointerMap` `NativeWordMax() - 8` overflow guards with wrapping
  multiply/add checks. `word` lowers as a signed Cranelift integer, so the old bound was
  negative under SignedGreaterThan and rejected every valid closure descriptor in the
  Linux JIT `isle_adapter` fixture (CYB-129).
- Lower unresolved Path call receivers as `CallLowering::Dynamic` (matching Member), so
  extern contract members and imported helpers no longer abort ISLE emission with
  unavailable `call_lowering` (CYB-129 Rust/Corelib gate).
- Rewrite canonical `ValidateTypeDescriptor` / `ValidatePointerMap` / `IsValidObjectAlignment`
  with explicit if/else so post-if statements remain reachable after CYB-129 merge lowering.
- Reuse the existing canonical runtime syntax session during sequential native runtime-kit
  publication, allowing debug and release kits to be built by one process without source-unit
  reassignment while retaining compiler-owned source authority (CYB-124).
- Fail syntax-module lowering closed when `[Export]` is attached to a non-public function, rather
  than silently omitting the invalid export from the AOT/JIT metadata surface (CYB-119).

### Removed

- Remove the no-op legacy Rust dispatch bootstrap and its bridge/test callers; host overrides
  now begin only through the manifest-owned `beskid_register_handlers` ABI entry point (CYB-86).
- Remove the retired public Codegen HIR/`Lowerable` facade (`LoweredProgram`, seven
  `lower_*` service entry points, and the root `Lowerable` re-export); public callers must
  cross the generation-safe `CodegenInput` → syntax ISLE boundary (CYB-111).

### Added

- Add generation-safe, artifact-owned closure static plans for descriptor, pointer-map, and
  allocation-request storage. Plans derive the complete object layout and capture map indexes
  from current syntax facts plus the canonical ABI manifest, reject stale/stack-reference facts,
  and deliberately carry no TLS or root-frame value (CYB-128).
- Lower capture-free, zero-argument `spawn (() => ...)` expressions through current-generation
  closure facts into a syntax-owned helper entry and the canonical ABI-v5 fiber trampoline.
  Capturing entries remain fail-closed pending the allocated/rooted closure-environment leaf
  (CYB-121).
- Publish the canonical closure-environment allocation, capture-store, and TLS root operations
  as manifest-derived ABI-v5 exports with checked-in bindings and provenance coverage; captured
  closure lowering can import only these exact runtime symbols (CYB-122).
- Lower capture-free immediate lambda calls directly through generation-safe closure facts and
  generated ISLE, binding proven argument slots in the caller without allocating a runtime
  closure; captured or bound closure calls remain fail-closed pending the ABI-v5 environment path
  (CYB-77).
- Add the native Windows COFF platform-object path for ABI-v5 runtime publication: MASM-backed
  virtual allocation/release, Windows TLS helpers, `.obj` staging, and a regression that requires
  the normal platform publisher to produce the static archive, DLL, and CYB-112 import library
  (CYB-116).
- Add a native ABI-v5 runtime-kit matrix staging command: it builds canonical
  debug and release artifacts into disposable inputs, derives provenance from
  native symbols, atomically publishes one exact `build-matrix` prefix, and
  exercises installed-prefix JIT and AOT smoke targets (CYB-83).
- Lower direct non-capturing zero-argument `spawn Entry` expressions through syntax facts into
  a generated ABI-v5 entry trampoline and canonical `fiber_spawn_with_cancel_slot` dispatch;
  lambda, capture, and argument-bearing spawn forms remain fail-closed (CYB-77).
- Parse fenced `code` expressions as their dedicated syntax kind and retain an explicit,
  span-bearing generated-ISLE rejection regression, replacing the CodeString CYB-81 inventory
  placeholder with verified evidence.
- Expand the parsed-project → CodegenInput → ISLE → verified-CLIF harness with nested direct
  calls, while/break/continue, if/else, lambda/spawn closed failures, range-for fail-closed
  coverage, and an explicit production-path assertion that HIR/`Lowerable` drivers stay retired
  (CYB-99).
- Make the syntax/ISLE inventory bijective for classification evidence: every IsleLowered kind
  names a verified CLIF regression, the unsupported roster is an explicit constant equal to
  classify, concurrency forms (lambda/spawn) keep span-bearing rejection fixtures, and remaining
  host-composition/try/code-string gaps are recorded as CYB-81 Codex blockers.
- Promote `MethodDefinition` to a production-supported `NodeKind::MethodDefinition` item at the
  generated ISLE boundary (no FunctionDefinition child-index alias).
- Cover multi-function syntax assembly failures at the module boundary, including deterministic
  attribution to the failing function rather than a sibling item.
- Catalogue every expanded-syntax node kind at the generated ISLE boundary with an exhaustive,
  deterministic lowered, structural, or unsupported-operation classification.
- Lower Corelib soft string builtins (`__str_len`, `__str_slice`, …), string concat/eq from
  interpolation desugar, and `string[index]` byte reads through syntax ISLE dispatch rules, so
  Foundation `Testing/Assert.bd` and `Core/String/String.bd` no longer fail with
  `MissingRuleOrFact` on the syntax-only path.
- Add opt-in `BESKID_COMPILER_TRACE=1` syntax-ISLE records for source keys, AST spans,
  call/import facts, selection failures, and CLIF emission timing in Corelib CI diagnostics.
- Authorize only the compiler-owned Foundation `Testing/Assert.bd` identity to lower
  `__panic_str` as the `panic_str` Corelib service; copied source remains ordinary syntax and
  emission declares only services reached by authorized calls.
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
- Share one ABI-v5 installed-prefix and host-target discovery helper across JIT, AOT, and native-host
  publishers (`BESKID_RUNTIME_PREFIX` or `<prefix>/bin/<tool>`), with exact `abi.json` coordinate paths.
- Stage the exact host debug runtime kit into the CLI install prefix from the E2E harness before JIT
  CLI tests run, while leaving missing/tampered kits fail-closed for all other consumers.
- Cover missing-manifest, wrong-target, hash-mismatch, and empty-prefix Engine fail-closed paths for
  the exact installed ABI-v5 kit route.
- Add `prepare_jit_entrypoint` / `prepare_jit_module` / `prepare_syntax_front_end` helpers so JIT and
  REPL consumers share one CodegenInput → ISLE prepare route with semantic diagnostics enabled.
- Cover CodegenInput JIT `run_entrypoint` missing-manifest and tampered-shared-library fail-closed
  regressions, plus REPL `ReplSession::try_new` missing-kit and tampered-kit fail-closed evidence.
- Bind LSP documentation actions to generation-safe syntax documentation facts (declaration
  span/kind, parameter/generic/return shape, leading doc span/text) derived from the current
  buffer's expanded AST, with stale-buffer and refresh regressions.
- Bind LSP diagnostics publish/refresh to generation-bound `syntax_diagnostics` facts on the
  current buffer revision, with stale-typed-generation fail-closed and no-analysis regressions
  (CYB-103 / CYB-65).

### Fixed

- Migrate stale `beskid_tests` export and Mod-rebuild coverage from deleted Codegen facade APIs
  to the syntax-owned `prepare_jit_module` and syntax registration routes, preserving exported
  artifact metadata and native Mod descriptor dispatch without an HIR compatibility fallback
  (CYB-118).
- Preserve `[Export]` metadata on all production prepared-syntax module artifacts so JIT and AOT
  publication retain the declared C ABI and linker symbol without reviving the retired HIR facade
  (CYB-115).
- Recognize methods owned by a conforming type definition when validating contract
  implementations, so the syntax-owned mod rebuild path no longer reports valid
  `Analyzer.Analyze` implementations as missing (CYB-117).
- Classify HIR-free gate dispatch evidence by ABI boundary: canonical ABI-v5
  manifest/ISLE routes are reported separately, while retired Rust-runtime
  dispatch and archive/profile fallback symbols remain release blockers
  (CYB-114).
- Collapse the callable-signature syntax-query guard so the compiler Rust clippy gate passes
  with `-D warnings`, without suppressing the lint (CYB-110).
- Lower parsed `range(...)` accumulator loops through generation-bound range and mutable-local
  assignment facts, rejecting immutable or stale write authority without an HIR/`Lowerable`
  fallback (CYB-80).
- Fail close canonical closure-environment descriptor registration before allocation or rooting:
  reject null requests/descriptors, non-power-of-two alignment, unaligned/out-of-bounds pointer
  offsets, and arithmetic overflow; execute valid rooting plus invalid-descriptor regressions on
  the Linux x86_64 runtime gate (CYB-109).

### Changed

- Produce Windows COFF static archives and DLL import libraries through the native `lib` and
  `link` tools, publishing the exact `beskid_runtime_import.lib` coordinate required by ABI-v5
  runtime kits (CYB-112).
- Remove the unused direct `beskid_runtime` dependency from `beskid_codegen` (CYB-113).
- Migrate the AOT mod-artifact object-compilation fixture from the retired HIR/`Lowerable`
  driver to the authoritative prepared syntax → `CodegenInput` → ISLE boundary (CYB-107).
- Migrate `beskid_engine` integration tests off retired HIR `lower_source` / `lower_program` drivers
  onto the sole CodegenInput → ISLE + exact ABI-v5 kit route; REPL snippet prepare uses the same
  shared front-end helper.
- Reload REPL/`Engine` sessions through the same validated exact kit selection
  (`Engine::reload_runtime_kit`) instead of reconstructing from the process install prefix.

- Consolidate the ABI-v5 native compiler worktree into `main`; conflicting prototype code keeps
  the newer canonical implementation while the complete worktree history remains reachable.
- Preserve the unfinished syntax-composition, runtime-authority, and generated ABI prototype in
  the consolidated 0.4 history while keeping newer canonical implementations at conflict sites.
- Render lower-spine type mismatches with the source-level type names retained by the partial type result.
- Resolve syntax-only qualified members only through the current import binding and explicit
  public `use`/out-of-line-module routes, including generated child modules; private terminal
  functions, types, and enums no longer escape their declaring module.
- Derive generic call ABI substitutions from explicit terminal or nominal-receiver type
  arguments, and reject bare generic qualified calls without source specialization.
- Resolve explicit nominal parameter and let receiver method calls through one generation-safe
  syntax fact, including their receiver ABI argument and ISLE local-slot lowering.
- Reject imported generic nominal static calls that omit receiver type arguments across syntax
  lowering, ABI selection, and generic specialization, while retaining explicit receiver and
  terminal-method instantiations.
- Register compiler-authorized Corelib syscall services per exact embedded source unit within
  multi-unit prepared syntax assemblies, leaving every sibling and forged source unprivileged.
- Preserve strict `_i32`, `_i64`, and `_u8` literal suffixes while allowing a bare integer
  argument to inherit an exact generic-call ABI only when its source magnitude fits.
- Lower that proven bare integer through the selected call-parameter ABI in generated ISLE,
  including nested Corelib assertion calls over `i64` results.
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
- Reject HIR/`Lowerable` codegen drivers (`lower_source*`, `lower_from_front_end`,
  `lower_program*`) with an explicit retired-path error so production assemblies must use
  `CodegenInput` plus `lower_syntax_*` / `lower_prepared_syntax_*`; multi-unit parsed-project
  harnesses prove stock-verifier-clean ISLE emission without a legacy fallback.
- Extend generation-safe `ClosureCapture` facts with capture mode (`CaptureStorageClass`) and
  first use-site span so `closure_environment` / spawn capture sets cover nested closures and
  shadowing without legacy analysis snapshots (CYB-96 / CYB-16).
- Normalize empty-arg `spawn Entry()` sugar to the entry path in `spawn_target` /
  `spawn_legality`, reject `spawn Entry(args)` with `CalleeArgumentsUnsupported`, and cover
  transferable vs mutable stack-escape capture legality with stale-generation rejection
  (CYB-104 / CYB-17).
- Route engine and AOT install-prefix / host-target lookup through the shared `beskid_abi::runtime_kit`
  authority instead of duplicated private helpers.
- Drop LSP `Document.analysis` / `DocumentAnalysisSnapshot` ownership from document lifecycle;
  documentation actions and refresh now use only generation-bound syntax documentation facts.
- Route LSP diagnostics publish/refresh through syntax diagnostic facts and generation-safe
  prepare queries; stale typed generations fail closed to parse/structural diagnostics for the
  current buffer instead of EntryOnly prepare-spine reuse.
- Canonical bootstrap `ThreadAttach`/`ThreadDetach` own a dedicated `BeskidTlsState`
  allocation (manifest size 32) so TLS root-frame offset 8 no longer collides with
  `BeskidRuntimeState.current_thread`; `ProcessInit` stamps `abi_version = 5` without
  installing RuntimeState into TLS (CYB-97 / W5.1 lifecycle prerequisite).

### Removed

- Remove the public runtime-kit-bypassing `emit_library_pair` AOT API. Native runtime publication
  now enters only through the host context/platform emitters used by the canonical runtime-kit
  builder.
- Remove the obsolete Rust language-handler regeneration hook, which targeted the retired
  runtime.v1 manifest rather than the canonical ABI-v5 contract.
- Remove the borrowed `FrontEndLowerInput` / HIR-only entrypoint codegen boundary.
- Remove AOT prebuilt-archive and standalone fallbacks, runtime link profiles, and host-archive lookup.
- Remove legacy Rust runtime registration, Engine-owned Rust runtime state, scheduler/TLS wrappers, and JIT `std`/`minimal` profile selection.

### Fixed

- Peel structural `ElseBranch` wrappers in syntax-fact child resolution so production if/else
  arms lower through ISLE without a HIR/`Lowerable` fallback.
- Preserve each generated-ISLE verification failure's originating expanded-syntax key and render
  its deterministic source path, generation/node identity, construct, and range through module
  diagnostics.
- Refuse a fixed `item_abi_signature` for generic function declarations so module emission
  registers call-derived `SpecializedItem` identities (including zero-argument factories whose
  nominal return type collapses to POINTER).
- Keep direct call lowering for nested generic calls that forward an enclosing type parameter
  (`CreateWithOptions<T>` inside `Create<T>`), so reachability and specialization collection stay
  connected for Corelib Channel/Console factories.
- Classify generic syntax module items before selecting ABI specializations, omitting generic
  type and enum declarations that have source layout facts but no executable ISLE body.
- Require an exact compiler-owned lexical source path before granting Foundation panic-service
  authority, so symlinked `Testing/Assert.bd` sources remain ordinary syntax.
- Follow parsed public module declarations in import-closure assembly while keeping use-path
  completion candidates scoped to the next unqualified module segment.
- Preserve complete logical module names for assembled `.generated/*.g.bd` units so syntax
  consumers do not register `Generated` modules under a truncated name.
- Terminate reachable syntax-ISLE control-flow merge blocks after one-arm `if` statements and
  preserve valid unreachable merges when both arms return.
