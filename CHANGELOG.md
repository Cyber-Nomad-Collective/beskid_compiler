# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) when applicable.

## [Unreleased]

### Fixed

- Allow only the Mach-O linker-generated `dyld_stub_binder` import when
  auditing the exact macOS shared runtime, preserving fail-closed provenance
  checks for undeclared runtime and application dependencies.
- Complete Linux x86-64 fibers through Cranelift tail transfers, preserving the
  scheduler's return stack while retaining one generated owner for completion
  state and one canonical System V signature for the manifest-owned context
  switch; share frame-pointer-preserving ISA settings across JIT and AOT so the
  x64 backend can emit those transfers.
- Materialize virtual source labels such as `<repl>` and `<memory>` under the
  portable `main.bd` leaf, allowing REPL type-checking and evaluation to run on
  Windows while retaining one isolated scratch root per lowering request.
- Re-enter x86-64 fiber return trampolines through an assembly alignment bridge
  on System V and Windows x64, preserving each manifest-owned calling
  convention's stack alignment and Windows home space after the fiber entry
  returns.
- Make ABI-v5 builtin generation idempotent across LF and CRLF checkouts,
  preventing Windows builds from appending the 49 manifest-owned declarations
  twice and reporting duplicate semantic items in compiler and REPL entrypoints.
- Isolate every synthetic codegen source in its own collision-checked scan root,
  preventing workspace discovery from ingesting sibling or stale scratch `.bd`
  files created by other lowering calls and processes.
- Return completed generated fibers through the ABI-installed scheduler return
  trampoline, keeping current-fiber clearing and context switching in one
  architecture-owned path and preventing Linux x86-64 spawned lambdas from
  resuming through duplicated generated context-switch logic.
- Read linked-image export and import directories alongside ordinary and
  dynamic symbol tables, so stripped PE DLLs retain exact ABI-v5 provenance
  validation instead of appearing to have no exports or dependencies.
- Resolve the manifest-owned fail-closed trap intrinsic in the executable
  canonical-runtime closure harness now that validated fiber completion is part
  of its transitive scheduler dependency slice.
- Declare the Windows environment adapter's `SetLastError` dependency in the
  canonical ABI-v5 manifest and generated runtime contract, allowing exact
  runtime-kit provenance checks to accept the intentional `kernel32` import
  while continuing to reject undeclared dependencies.
- Complete generated fiber entries through the canonical scheduler state transition,
  preventing runtime shutdown from rejecting successfully drained spawned lambdas
  whose terminal state had been written with a duplicated raw enum discriminant.
- Keep the extern frontend regression aligned with parse recovery: unsupported
  `ref` parameter syntax remains rejected before codegen without coupling the
  test to an obsolete recovered module-path diagnostic.
- Allow only the standard ELF shared-linker startup imports when auditing the
  exact Linux context library, while keeping runtime, application, dynamic-TLS,
  and static-archive dependencies fail-closed under the centralized provenance
  policy.
- Resolve process-linked Unix externs through `libc::RTLD_DEFAULT` and its
  `dlsym`/`dlerror` contract, preventing Linux JIT workers from using Darwin's
  sentinel handle and crashing on standard functions such as `sched_yield`.

- Treat `Never` as the bottom type when joining match-expression arms and terminate
  effect-only Never arms without inventing a result value, restoring Fiber.Join
  lowering. Initialize Core.Args from one explicit host-owned JIT vector through
  the manifest-selected UTF-8/UTF-16 runtime handoff, publish that adapter from
  native runtime kits, and present matrix target completions through one shared
  live/drain event path. Make exact-kit validation the single authority for
  manifest-approved platform imports such as libm. Align the remaining grammar
  fixtures with canonical `mod` and `u32` syntax and pin the consolidated
  Corelib Args test authority.
- Restore the compiler-pinned Corelib runtime-authority implementation for
  atomic typed channel receive, split fiber join status/value, descriptor-backed
  arrays, and the reusable text/compiler-SDK surface, eliminating drift between
  embedded source bytes and the ABI-v5 service table.
- Restore the strict workspace Clippy gate on Rust 1.98 by consolidating enum
  match materialization inputs into one semantic context and using the direct
  collection membership and conditional forms required by current lints. Keep
  codegen fixtures and the JIT callable helper compliant with the same gate by
  removing redundant conversions and placing non-test items before test modules.
- Emit standard `property` semantic tokens for BSOL assignment and inline-map
  keys while retaining `namespace` tokens for block kinds, keeping VS Code and
  Zed semantic highlighting aligned with the shared tree-sitter grammar.
- Keep the `just replace` runtime-kit staging paths bound to Just's resolved
  workspace root instead of referencing an undefined shell variable after a
  successful release build.
- Lower manifest-authorized `raw_byte_store` calls to exact one-byte stores,
  preventing adjacent decimal digits and other byte-buffer writes from being
  overwritten by the high zero bytes of wider integer values.
- Keep Win32 `ReadFile` in the exact ABI-v5 platform-import allowlist without
  misclassifying its leading `R` as Rust v0 mangling, while continuing to
  reject symbols that actually demangle as Rust or match forbidden runtime
  families. Restore the CRT-free Windows runtime boundary by removing the
  stale `strlen` platform import that had drifted back into the canonical
  manifest despite having no Windows runtime reference.
- Declare source-callable runtime builtins explicitly in ABI v5, beginning with
  `__str_len`, and carry that authority through one typed call-lowering fact.
  Public builtin signatures and symbols now come from generated manifest data;
  privileged Corelib services remain capability-gated, stale ABI-v4 builtin
  shapes cannot request imports, and external callees are collected in one
  traversal. Correct `f64` soft-builtin generation to use floating ABI slots.
- Keep imported-member completion available while a Beskid expression is
  temporarily incomplete, so partial members such as `Output.Wri` still suggest
  `WriteLine` in LSP editors. One workspace Salsa store now serves per-document
  ordered edits and concurrent read-only handles; each document owns a bounded
  dependency completion surface keyed by exact import path and alias, while
  current-buffer syntax facts and versioned diagnostic publication remain
  generation-strict. Open buffers retain their own source identity instead of
  inheriting the configured target entry, and incomplete semantic resolution
  does not erase recoverable syntax facts. Entry-only feedback is immediate;
  full diagnostic analysis is coalesced behind a 120 ms idle debounce and runs
  after releasing Salsa writer and LSP state locks in a cache-isolated mode that
  does not materialize generated project outputs.
- Advertise `@` as an LSP completion trigger so standalone BSOL editors show
  the existing `@schemaless` suggestion as the annotation is typed.
- Frame prepared-matrix worker events separately from Beskid program stdout,
  use JSON-portable 64-bit timing fields, and reset generation-bound compilation
  state between independent targets, so stdout-writing and sequential Corelib
  tests cannot corrupt or collide with later supervisor work.
- Align the managed-string Corelib write and panic service signatures with
  their runtime adapters, forwarding the managed string pointer intact while
  reserving UTF-8 data-and-length extraction for raw byte-view services, and
  service writes synchronously when a JIT host is not executing inside a fiber.
- Resolve direct-return empty array literals from the enclosing generic
  specialization, giving each concrete instantiation distinct, correctly typed
  array metadata while leaving ambiguous empty arrays rejected.
- Keep native-runtime provenance exact without mistaking Win32 `R*` imports for
  Rust symbols, remove the retired Windows `strlen` dependency, and make the
  isolated Core.Args adapter fixtures satisfy the managed-string constructor
  boundary used by environment services.
- Expose manifest-owned scheduler stack-check seams to ordinary generated spawn
  trampolines, so application artifacts retain bounded-stack enforcement while
  canonical runtime builds reuse the same local definitions.
- Cover the `pckg pack --skip-docs` contract end to end, requiring prepared library API docs to
  remain byte-identical in the artifact and its manifest documentation pointer.
- Reuse the shared scalar-boundary adapter for direct-call parameters so semantically authorized
  integer width changes also lower inside nested calls instead of failing at the enclosing call.
- Replace the local `~/.beskid` toolchain from one validated, exact-version bundle using a
  same-filesystem staged swap with rollback, removing stale runtime, corelib, and package files while
  preserving non-toolchain user data and the prior installation when publication fails. `just
  replace` now reports missing aggregate-superrepo prerequisites before starting a build.
- Resolve fully qualified public enum types through their exact assembled module so dependency
  functions can lower contextual generic constructors without a redundant local import.
- Contextualize unsuffixed integer literals from their exact field position in multi-payload enum
  constructors, preserving the declared ABI width without implicit numeric conversion.
- Authorize environment lookup only from the exact compiler-embedded Console terminal
  facade, completing fail-closed lowering for terminal-dependent console modules.
- Preserve a generic nominal type's parameter environment inside its methods so aggregate literals
  and zero-argument generic factories lower concrete scalar field widths consistently.
- Authorize terminal-size lowering from the exact compiler-embedded Linux, macOS, and Windows
  console platform facades while copied paths and altered source bytes remain fail-closed.
- Build local VS Code packages with the shared exact editor version, verify the bundled language
  server reports that version, and reinstall the resulting VSIX into every available supported editor.
- Materialize artifact string addresses with non-colocated relocations so the arm64 JIT does not
  require literal data to remain within 2 GiB of code.
- Resolve a one-segment type imported from its homonymous module to the public type declaration,
  preserving generic iterator signatures in Corelib array facades without loading dependency bodies.
- Resolve dependency declaration annotations through one seeded, source-scoped resolver without
  retaining dependency body facts, restoring fast entry type-checks and keeping entry locals attributed
  to the entry source.
- Preserve each dependency unit's import scope in entry-resolution type facts so qualified Corelib
  facades retain their declared parameter and return types.
- Make the native runtime-kit CLI smoke independent of test-only Corelib packages while still requiring
  its computed process result to be exactly 42, and retain that result in runtime-kit evidence.
- Refresh ABI-v5 bootstrap expectations from the checked-in canonical manifest after the intended
  poll runtime, composition, callback, filesystem, environment, worker, and scheduler contracts were added.
- Refresh an installed bundled corelib when its content fingerprint differs even if its semantic
  version is unchanged, preventing stale same-version templates from hiding newly shipped APIs
  from compiler and editor resolution.
- Allocate managed aggregates and closure environments inside the descriptor-traced GC heap, and
  align the host runtime's type-descriptor decoder with the canonical ABI-v5 field layout; retain
  concrete string equality and call-result field layouts through nested generic specialization.
- Derive inferred-let ownership from the initializer when the physical value width aliases a
  native pointer, and make storage/match CLIF tests select their intended caller and assert
  structural control flow instead of obsolete optimized instruction shapes.
- Prioritize generation-bound stored-lambda call facts over broader collection-operation probes,
  preventing local zero-capture callables from being misclassified during ISLE lowering.
- Preserve syntax-generation identity in canonical-runtime test assemblies, emit scheduler
  trampolines only for complete selected scheduler slices, and keep descriptor-validation
  fixtures synchronized with their `TypeDescriptorFlags` dependency.
- Let non-local paths fall through the optional generic-local managedness probe, preserving
  scalar authority for resolved integer constants; align Core.Args import tests with exact ABI-v5
  symbols and the always-admitted string baseline.
- Preserve source-level ownership for concrete generic enum match payloads, so array and nominal
  bindings lower as managed references without treating every pointer-shaped ABI value as managed.
- Assert generic string equality and inequality against the manifest-authorized `str_eq` call and
  its zero-result predicate, removing stale numeric dispatch-tag assumptions from ABI-v5 tests.
- Materialize generic enum-match layouts, payload ownership, and bindings from each exact
  call-derived item specialization, so shared `Result<TValue, TError>` predicates and transforms
  lower without guessing pointer-shaped generic identities or leaking substitutions between
  multiple instantiations of the same declaration.
- Resolve the runtime-kit staging script from Just's compiler workspace path during
  `just replace`, avoiding an unbound shell variable after a successful release build.
- Preserve resolved integer constants as binary type authorities over unsuffixed literals, using
  normalized sibling keys so compound `word` expressions retain their representation when passed
  through exact call boundaries.
- Contextualize resolved integer constants through the same range-checked binary-operand seam as
  unsuffixed literals, so signed comparisons retain their declared sibling type instead of
  inheriting the constant's standalone `word` representation.
- Preserve and validate unary operand semantic types through both concrete syntax nodes and
  expression wrappers, so signed negation inside nested comparisons lowers without weakening
  fail-closed operator checks.
- Normalize grouped expressions through the shared transparent-expression seam so nested
  arithmetic such as `(tail + 1) % 32` retains `word` unsigned lowering instead of failing
  closed at the wrapper.
- Isolate the Linux guarded-stack behavior harness at function-section granularity, so it links
  the production stack adapter without requiring unrelated Corelib string services from the
  monolithic platform-host object.
- Enforce exact equality between parsed Beskid runtime exports and ABI-v5 manifest signatures,
  including mutable and single-line declarations, and remove stale `array_len` authority from
  Base64 and Hex sources that no longer call the service.
- Balance GC-managed local roots through explicit lexical and control-flow scopes, preserve
  source-managed identity for array-valued enum bindings, and close effect-position block scopes
  at their lexical boundary so branch-only and match-bound roots cannot contaminate alternate
  returns, loop transfers, or following statements.
- Resolve source-authorized Corelib services through one fail-closed ABI-v5 binding seam, including
  materialized process arguments, scheduler clock/yield calls, Linux terminal sizing, and canonical
  Fiber join/cancel signatures. Channel try-receive now claims and removes exactly one FIFO value in
  the status observation, retains managed payloads in a rooted receipt until typed consumption, and
  prevents competing receivers from observing the same message.
- Keep Corelib aggregate and shard manifests bound to their containing workspace, preventing an
  installed implicit `Std` from introducing a second divergent declaration set into checkout builds.
- Report and evaluate filtered prepared-matrix runs against their selected target count while
  retaining the complete manifest inventory as the release-eligibility denominator.
- Close the isolated prepared-matrix worker's standard input so input tests observe deterministic
  end-of-file instead of inheriting an open terminal or pipe and blocking indefinitely.
- Compute match exhaustiveness recursively across repeated nominal enum patterns, so complete sets
  of nested variants cover their enclosing payload while missing nested variants still fail closed.
- Preserve every enum payload field in source order through physical layout and recursive match
  lowering, including multi-field nominal patterns such as `MatchSpanList::Cons(_, _)`.
- Grant ABI-v5 service authority to the exact compiler-embedded concurrency facades, so Channel,
  Mutex, Hub, and WaitGroup handle creation and operations lower as canonical native calls while
  identical builtin spellings in application source continue to fail closed as dynamic calls.
- Reuse one integer boundary adapter for returns and explicit local storage, allowing proven
  narrower call results such as Console Style's `i32` sizes to initialize `i64` locals.
- Partition value block expressions into executable prefix statements and one final value while
  lowering statement-position blocks as complete effect sequences, including empty match arms.
- Preserve recursive source-type identity when specializing inferred and nested generic calls,
  including aggregate fields, direct-call results, enum pattern bindings, and applied enclosing
  parameters such as `MapEntry<TKey, TValue>`, without exposing source identity in serialized facts.
- Treat an existing local binding as the authority for temporary-root decisions, so specialized
  nested nominal values pass through `Array.Append` without redundant path reclassification.
- Materialize generic aggregate layouts from only their layout-relevant type parameters, allowing
  phantom applications such as `ArrayIter<T>` to retain concrete record fields without weakening
  source-level specialization identity or accepting unresolved storage shapes.
- Keep GC-managed compiler locals and append payloads rooted across collecting calls, publish grown arrays through owner-correct barriers, release local roots on every function exit, normalize public array references at the GC marking boundary, and store collector mark states as raw words rather than managed enum objects.
- Derive enum-match binding lifetimes from their authoritative payload shapes and apply enclosing
  generic source substitutions when deciding whether call operands need temporary GC roots, so
  returned nested matches and specialized collection mutations lower without syntax heuristics.
- Resolve proven extern-contract calls from their declared ABI signatures without requiring a
  synthetic generic specialization identity, including imported results used by comparisons.
- Materialize scheduler function and string-literal addresses with range-independent JIT relocations so macOS ARM64 allocations cannot overflow Cranelift's `ADRP` window.
- Restore compiler and BSOL authoring manifests and lockfiles after release artifact
  builds succeed or fail, so local release staging has the same rollback guarantee
  as package publication.
### Changed

- Bind BSOL semantic-token candidates to each LSP document generation so
  `.bproj`, `.bws`, and standalone `.bsol` requests encode stored syntax facts
  without reparsing the buffer.
- Carry fixed-width unsigned `u32` as a first-class primitive through parsing,
  semantic facts, ABI services, layouts, casts, CLIF lowering, and JIT display;
  unsigned comparison, division, remainder, and extension no longer alias
  signed `i32` behavior.
- Stamp the complete local Cargo dependency closure and reachable first-party
  BSOL workspace shipped by CLI, LSP, and updater, plus bundled corelib,
  first-party template packages and identities, and first-party package
  dependency constraints from one exact release version. All three shipped
  executables expose that identity through `--version`; release staging fails
  closed when its dependency graph or any inventoried surface drifts. The bundled
  pest generator schema is stamped with the bundle version while remaining
  outside the registry publication set.
- Discover runtime kits and bundled corelib workspaces from the canonical
  `<prefix>/bin`, `<prefix>/lib`, and `<prefix>/beskid_corelib` installation
  layout, while retaining `BESKID_RUNTIME_PREFIX` and `BESKID_CORELIB_ROOT` as
  optional overrides. Local replacement installs now stage the native runtime
  kit and bundled corelib into the same prefix as the CLI.
- Persist registry-owned package descriptions, categories, repository and
  website links, tags, and icons in both memory and PostgreSQL backends, and
  project that canonical metadata through public package summaries and detail
  responses instead of synthesizing placeholders.
- Preserve parser-generator signatures against the reusable
  `TextParseResult<string>` surface while representing each success as one
  nominal product payload.
- License the compiler, tooling, reusable crates, and embedded runtime under
  Apache-2.0 while scoping AGPL-3.0-only to the runnable pckg server package;
  preserve vendored dependency licenses and declare Cargo SPDX metadata.
- Carry Apache license and notice files in native runtime kits and the
  compiler-embedded corelib snapshot so redistributed artifacts retain their
  applicable legal terms.

- Make recursive staged-analysis helpers stateless associated functions,
  removing unused rule receivers and keeping the workspace lint gate clean.

- pckg now accepts the trusted identity headers forwarded by the Authentik
  proxy outpost when `SHELL_AUTH_MODE=authentik`, including the configured
  administrator and moderator group rules.
- pckg package summaries now derive required `packageKind`, nullable template
  metadata, and canonical dependencies from the immutable validated artifact
  manifest. Published manifests are persisted with each version and parsed by
  the artifact crate's single metadata abstraction; incomplete legacy rows and
  unpublished records are not projected as invented package kinds.
- pckg production startup now requires
  `PCKG_RELEASE_PUBLISHER_KEY_SHA256` and atomically reconciles it into one
  deterministic `release:github-actions` automation principal with read and
  publish scopes. Only the lowercase SHA-256 digest enters server
  configuration; malformed, missing, or colliding credentials fail startup.
- pckg packaging now uses one path-independent artifact dependency contract:
  exact registry dependencies are read from the staged `package.json` release
  plan, source-only `.bproj` path declarations are rewritten in collected
  artifact bytes, and the same canonical dependency list is emitted in the
  artifact manifest. Source workspace manifests remain unchanged.
- pckg packaging now emits canonical per-project artifacts only: template
  authoring manifests move from `.beskid/template.json` to artifact-root
  `template.json`, `.bproj` replaces the removed `Project.proj` shape, and
  kind-specific validation admits aggregate libraries without sources and
  template scaffold roots without a synthetic `src/` tree. Library metadata
  advertises API docs only when the artifact actually contains them, while
  `pckg pack --skip-docs` supports pre-generated or declaration-only release
  inputs.
- pckg Rust backend: publication routes now authenticate an active,
  `publish`-scoped `Authorization: Bearer bpk_*` key through the registry
  store. The shipped publisher client uses that same standard bearer transport
  rather than a configurable custom key header. A bearer key never combines
  with `Remote-*` headers; invalid, revoked, or inactive keys are rejected
  without falling back to a forwarded or mock identity. Valid publisher keys
  work when browser-session authentication is intentionally unset.
- pckg Rust backend now accepts package publication only as artifact-bound
  multipart `version`, `checksumSha256`, and `.bpk` bytes on
  `POST /api/packages/{name}/versions`. The metadata-only JSON request and
  detached raw-artifact upload route are removed.

- Centralize exact ABI-v5 target-triple resolution in
  `TargetMetadata::for_triple`, preserving existing CLI, AOT runtime-kit, and
  runtime tooling diagnostics at their boundaries.

- Route generated spawn and lambda trampoline signatures and closure-capture types through the
  ISLE adapter's canonical semantic-to-CLIF mapping, removing duplicate local conversion rules.

- Route statement and expression parse-recovery insertion positioning through the
  shared syntax-boundary primitive, removing duplicate local helpers without
  changing repair policy.

- pckg Rust backend: replaced the Auth Hub handoff/JWT session model with
  Authelia forward-auth. The server is now a resource server that trusts
  Authelia's `Remote-User`, `Remote-Email`, `Remote-Name` and `Remote-Groups`
  headers (`SHELL_AUTH_MODE=authelia`), with a `SHELL_AUTH_MODE=mock` dev mode
  that mints a single configurable admin principal. Admin/moderator roles are
  projected from Authelia groups (`pckg-admins`, `pckg-moderators`) instead of a
  persisted role table; the `pckg_admin_roles` table and the
  `/api/admin/roles` grant endpoints are removed. Publisher verification,
  per-resource moderation grants and the package-review audit log remain
  registry-owned. API-key authentication for CLI publishing is retained.
  `/api/auth/session` now returns the Authelia-projected identity
  (`subject`, `email`, `displayName`, `groups`).

- pckg Rust backend: removed the community surface. The `beskid_pckg_community`
  crate and the `/api/community` router (profiles, boards, posts, comments,
  votes, follows, notifications) are deleted — NodeBB owns the forum. The
  publisher directory now derives from package owners plus administration-store
  verification, with no community profile dependency. Package-scoped reviews
  (`/api/packages/{name}/community-reviews`) are retained as simple package
  ratings, extracted from the community module into `beskid_pckg_store`'s
  `package_reviews` module.

- pckg Rust backend: relaxed subject validation to accept any non-empty
  trimmed identifier (Authelia usernames and carried-over `github:<numeric-id>`
  subjects), replacing the `github:[0-9]+`-only CHECK constraints and Rust
  validators across the store migrations and repositories.

- pckg Rust backend: removed the legacy ASP.NET Identity cutover. The
  `beskid_pckg_store` cutover module, the `pckg_cutover` binary and the
  `0002`/`0003` migration files are deleted; the .NET server is being retired
  and no Identity data is migrated.

### Fixed

- Preserve substituted enum payload types while recursively checking nested match patterns, so bindings
  such as `Result::Error(FsError::NotFound(path))` retain their concrete source identity.
- Recognize `_u8` integer suffixes during semantic type checking, keep embedded
  Corelib gates on the canonical threading module and current match grammar,
  and link the isolated Linux guarded-stack harness against its required
  canonical string constructor.
- Let canonical runtime syscalls complete through the worker pool when invoked
  from an attached host thread outside a scheduler fiber, while retaining the
  single copied-buffer worker implementation used by parked fibers.
- Frame prepared-matrix worker events independently from tested program stdout,
  so successful console and syscall tests cannot corrupt the supervisor stream.
- Serialize prepared-matrix timestamps and durations as bounded 64-bit
  milliseconds, matching JSON's supported integer wire representation.
- Preserve the first per-test diagnostic in matrix target reports instead of
  collapsing every failure to a count without actionable evidence.
- Lower each prepared executable in an isolated generation-bound query database,
  preventing shared dependency sources in adjacent matrix targets from being
  relabeled across independent frontend generations.
- Match named aggregate literal values to their canonical declaration-layout
  slots before lowering, including reordered fields, and contextualize
  unsuffixed integer fields from the exact applied generic layout.
- Preserve zero-sized unit enum payload effects through the same canonical
  statement/expression lowering path used by discarded expressions, and retain
  source-keyed failures for unsupported effectful unit shapes.
- Derive assignment storage ABI from one canonical local, aggregate-field, or
  indexed-array query path, so typed field writes lower without HIR recovery.
- Limit call-argument contextual ABI facts to fitting unsuffixed integer
  literals, preserving nested floating-point operands' declared types.
- Lower explicit signed and unsigned integer-to-`f64` calls through the
  canonical primitive numeric conversion fact, and route `f64` division
  through the shared division emitter without integer-only trap instructions.
- Preserve the exact nominal aggregate declaration carried by an enum-pattern
  binding, allowing syntax-only lowering to project fields from product
  payloads without reconstructing HIR types.
- Materialize applied generic aggregate layouts from source type arguments and
  enclosing item specializations, keeping scalar and pointer field shapes
  authoritative through literals, enum-pattern bindings, projections, and
  managed-object allocation.
- Project generic enum constructors through the explicit type annotation of a
  proven mutable-local assignment target, preserving enclosing substitutions
  for `Option::Some(value)` while inferred, immutable, qualified, and nonlocal
  assignment targets remain unavailable.
- Collect module-local declarations before imports and isolate each source's
  import aliases during assembly seeding, so a leading import cannot replace a
  same-named local function such as `Query.ArrayIterator.Current` with a
  homonymous dependency helper. Assembly seeding now also restores the caller's
  source identity after temporarily collecting dependency units.
- Resolve explicit call-site type arguments in their import scope, preserve
  recursively nested generic parameters in dependency type surfaces, and count
  type annotations, generic arguments, and aggregate constructors as import
  uses. Imported SDK records now retain nominal identity through generic array
  helpers instead of degrading foreign `T[]` signatures to unit/i64 fallbacks.
  Dependency surfaces also reconstruct module aliases with lexical inline-module
  scoping and reject ambiguous same-scope aliases, preserving exact facade
  parameter and return identities when dependency bodies are not resolved.
- Authorize canonical `Core.Collections.Array.Len` to import the ABI-v5
  `array_len(pointer) -> usize` service, so its compiler-owned body lowers
  through the declared runtime boundary instead of remaining a dynamic call,
  and make that runtime service read the logical length from the public array
  header rather than returning its placeholder zero.
- Preserve the enclosing generic specialization when lowering indexed array
  assignments, allowing canonical `Array.Set<T>` bodies to emit their checked
  typed store instead of losing the assignment result ABI fact.
- Keep a nested call expression's specialized result ABI authoritative over an
  enclosing call argument context, so comparisons such as
  `Array.Capacity<T>(values) >= Array.Len<T>(values)` lower as word values when
  passed to a boolean assertion.
- Deduplicate identical nominal declarations reached through both a direct
  import and a module re-export while retaining ambiguity for distinct types,
  allowing imported generic records such as `ArrayIter<T>` in typed locals.
- Track import use within each item's generic scope, so a same-named generic
  parameter no longer hides an otherwise-unused imported type from `W1503`.
- Make the JSON-RPC IntelliSense integration test wait for the completed
  workspace scan and versioned open-document diagnostics, avoiding stale
  disk-scan notifications before completion and hover assertions.

- Preserve the prepared program assembly's syntax generation in incremental LSP state so
  dependency-backed completion, definition, hover, and references no longer fail closed with
  empty facts after confusing cache revisions with syntax generations.
- Use the idiomatic boolean assertion in the pckg dependency-rewrite test so
  the release compiler gate passes with warnings denied.
- Return the validator's deterministic publication error so package-kind
  conflicts identify the conflicting archive file instead of being hidden by
  a generic invalid-artifact message.
- Retire each artifact's managed heap before unloading its JIT code and static
  type descriptors, then attach a fresh runtime state for the replacement
  artifact so later tests cannot dereference retired descriptor storage.
- Separate the source-owned `beskid_rt_v5_trap` export from its terminal
  platform intrinsic, preventing recursive trap dispatch, and preserve the
  full generation word when creating the first GC root handle so construction
  roots release cleanly at process shutdown.
- Mark ordinary aggregate and enum descriptors as fixed-size objects rather
  than variable-sized arrays, preventing valid collection instances from
  failing managed allocation.
- Reject uploaded `.bpk` artifacts whose project dependencies retain path/git
  sources or disagree with artifact-root `package.json`; template validation
  now parses `template.json` and enforces the v1 schema, package identity, and
  exact summary agreement.
- Align `beskid pckg upload` with the Rust registry's canonical per-package publication contract:
  read the immutable version from artifact-root `package.json`, always send multipart `version`,
  `checksumSha256`, and `artifact` to `/api/packages/{name}/versions`, and remove the obsolete
  `/publish`, version-bump, and detached-manifest client shape.

- Lower canonical `Array.Empty<T>` through specialization-scoped ABI-v5 managed-array
  descriptors and the rooted construction transaction, while keeping copied source and the
  historical size-only `__array_new` service path unauthorized.

- Keep imported generic receiver methods and static return signatures bound to
  their declaring source unit. Unit type surfaces now remain authoritative when
  dependency files reuse the entry file's byte offsets, preventing unrelated
  nominal types from replacing canonical `List<T>`/`Stack<T>` identities.

- Materialize generic collection method bodies from their exact call-derived
  specialization: implicit fields and `self`, nested generic calls, contextual
  `Result` constructors, and generic array indexing now retain their source-owned
  ABI facts through TypedProgram/Salsa lowering.

- Type the compiler-owned `__gc_collect` facade service as a pointer-width word so
  `Testing.Assert.CollectGarbage` can emit its authorized ABI-v5 import while copied or altered
  sources continue to fail closed.

- Keep the trimmed Rust pckg server image build independent of the compiler
  package client graph by moving the client-to-server bearer-auth contract to
  `beskid_tests_pckg` and removing the test-only client dependency from
  `beskid_pckg_server`.

- Always admit string runtime helpers (`str_new`, `str_from_i64`, `str_eq`,
  `str_concat`) as corelib service imports during ISLE lowering, even without
  the Corelib syscall capability. These services are emitted directly by the
  ISLE string context (literals, coercion, comparison, concatenation) and are
  fundamental operations, not facade services requiring capability authority.
  Previously, `corelib_service_symbols` returned an empty map when the
  capability was absent, causing `UnknownCallee` lowering failures for any
  source using string interpolation or comparison outside the canonical corpus.

- Eliminate double typing of call arguments in the generic inference path.
  The type checker previously typed each argument once for
  `record_generic_call_constraints` and again inside the
  `infer_generic_args_from_call` wrapper. The wrapper is removed and the
  already-computed argument types are passed directly to
  `infer_generic_args_from_call_types`, reducing duplicate `type_expression`
  calls and potential duplicate diagnostics.

- Link Windows executables through their validated Beskid entry symbol and
  console subsystem instead of implicitly requiring the CRT startup symbol.

- Read Windows shared-runtime exports and imports from the PE tables with
  `llvm-readobj`, while retaining `llvm-nm` provenance for static archives.

- Classify MSVC string-literal COMDATs and the COFF feature marker as local
  compiler metadata during runtime provenance checks without widening ABI exports or imports.

- Make the Windows ABI-v5 runtime DLL CRT-independent by using manifest-owned
  Win32 TLS APIs and literal trap lengths instead of `_tls_index` and `strlen`.

- Accept the intentional `42` exit status from the native runtime-kit CLI smoke program while continuing to reject every unexpected result.

### Added

- Provide parser-backed semantic tokens for `.bproj`, `.bws`, and standalone
  `.bsol` documents. BSOL block kinds use the existing `namespace` declaration
  token and assignment/map keys use the existing `variable` declaration token;
  invalid BSOL fails closed without partial semantic facts.

- Add opt-in anonymous OpenTelemetry export for Rust CLI/pckg flows:
  - `BESKID_TELEMETRY` now controls OTEL enablement (unless `OTEL_SDK_DISABLED` is set).
  - `beskid_cli` initializes the shared telemetry subscriber at process start.
  - Compiler diagnostics now emit warning/error tracing events during rendering for OTEL ingestion.
  - `beskid_pckg` command execution emits structured command/error events while preserving current CLI output.

- Lower the grammar-supported postfix `Result` propagation form (`value?`)
  through generation-bound Salsa facts and generated ISLE. Only the exact
  canonical `Result<T, E> { Ok(T value), Error(E error) }` definition and an
  identical enclosing `Result<T, E>` return instantiation are accepted; all
  lookalike, stale, and layout-incompatible forms fail closed.

- Derive declared-array index-assignment layout and result ABI from explicit
  `T[]` parameter or local annotations, enabling verified bounds-checked
  stores without treating compound assignments or inferred arrays as supported.

- Carry immutable generic specialization environments through semantic facts and module emission.
  Nested explicit generic calls now enter a declaration-instance worklist keyed by declaration and
  substitutions, while `ModuleEmissionSession` namespaces source artifacts and reuses declared
  callee handles for repeated long-lived-module emission. Source verification remains pending the
  scheduled Cargo/Clippy pass.

- Publish typed managed arrays only through the rooted construction transaction: the ABI header,
  allocation registry, and external construction root are established before the collector can
  observe the allocation. Generic module identities now include ordered substitutions, and session
  namespaces cover closure, aggregate, array, and string static data.

- Remove the unsafe unrooted typed-array allocator from the ABI-v5 manifest/runtime surface.
  Generic specialization identity uses a full length-delimited parameter encoding, and module
  session cache keys include linkage policy as well as source item identity.

- Capture Core.Args for AOT executables through generated native entry adapters:
  Linux x64 and macOS arm64 preserve `argv`, while Windows x64 captures `wmain`
  UTF-16 arguments and replaces malformed surrogates deterministically. The
  adapters retain copied `BeskidStr` values for process lifetime and expose only
  the manifest-owned `__args_count` and `__args_get` services.

- Generate manifest-owned ABI-v5 bindings for the exact `Core.Args`
  `__args_count` and `__args_get` services on Linux x64, macOS arm64, and
  Windows x64. Validation now rejects missing or duplicate target bindings,
  signature drift, duplicate services, and undeclared target OS imports, while
  checked-in Rust, C, JSON, and audit artifacts remain source-fresh.

- Authorize only the byte-identical canonical Foundation `Core/Args/Args.bd`
  source to import `__args_count() -> i64` and `__args_get(i64) -> string`
  through the existing source-scoped `CorelibService` path; copied, altered,
  symlinked, and user-authored spellings fail closed before CodegenInput/ISLE
  can select an ABI import.

### Removed

- Remove the pckg server's workspace-bundle publication route and its private
  batch-persistence contract. Release publishers now use the single canonical
  per-package artifact path for package creation, immutable version
  reservation, and `.bpk` upload.

- Purge the legacy ABI dispatch envelope, generated routes/tags/tables, Rust runtime/handler/host crates, differential package and features, and language-handler registration remnants. ISLE and codegen now import exact canonical Corelib service symbols and signatures directly through `CodegenInput` authority.
- Cut production engine execution over to the exact ABI-v5 native runtime kit only: remove Rust host registration, GC bootstrap, process-linked builtin fallback symbols, legacy host registration generation, and the retired `rust_fallback_handlers`/`arrays_backing` features. Rust runtime and host adapters now require an explicit differential-test feature, enforced by a retirement scan.
- Retire the HIR-based analysis and legacy Rust-codegen suites from
  `beskid_tests`; generated-syntax `CodegenInput` → ISLE regression suites are
  now the maintained codegen authority, and the HIR-free gate asserts that the
  retired suites cannot be reintroduced.
- Retire the unused `beskid_runtime_bridge` static archive package and its
  source-tree build/CI compatibility helpers; the retirement gate now rejects
  restoring the bridge package or workspace member.

### Changed

- Split the remaining oversized compiler integration-test authorities for compile planning,
  semantic facts, CodegenInput, ISLE lowering, and ABI-v5 manifest generation into focused
  modules with explicit shared-support seams and unchanged test inventories.

- Split the compiler's remaining large multi-responsibility Rust modules and the
  canonical Bootstrap, GC, and scheduler Beskid sources into explicit,
  acyclic facade graphs while preserving their public paths and runtime corpus
  order.

- Align JIT soft-builtin registration with the ABI-v5 Core.Args contract: JIT no
  longer supplies ambient process arguments, while numeric math exports remain
  registered through the runtime builtin module. Linux x64 terminal fallback code
  is now correctly target-gated for warnings-as-errors CI builds.
- Replace the remaining `lsp-latest` CLI install default with explicit
  `lsp-stable`/`lsp-unstable` channel usage in compiler toolchain defaults and
  tests so release consumers can opt into unstable builds by channel, not by
  ambiguous `latest`.
- Migrate AOT and parsed-project codegen tests to the production prepared-syntax
  → `CodegenInput` → ISLE boundary, removing test reliance on the retired HIR
  `lower_program` compatibility path.
- Prepare-spine frontend assembly projects `SyntaxProgramAssembly` as the IDE/query
  authority (`PreparedCompilation::syntax_assembly`); LSP lifecycle binds facts from
  `FrontEndTypedResult::syntax_assembly` (post-mod-rewrite entry), and
  `beskid_queries::syntax_program_assembly` strips HIR at the Salsa assembly boundary.
  `DocumentAnalysisSnapshot` remains CLI-doc-only (CYB-65).

### Fixed

- Keep nominal source signatures fail-closed when the scalar semantic type ID
  cannot represent their qualified identity, while retaining pointer lowering
  exclusively in ABI queries; compose enum-match arm types past inapplicable
  contextual-integer facts.

- Select floating-point addition for float operands and route signed division
  through the explicit integer-division-by-zero trap guard before emitting CLIF.

- Keep exact-kit JIT rejection tests outside the canonical ABI-v5 platform
  import allowlist by using a genuinely unapproved process-symbol witness.

- Restore readable syntax trace sites as ordinary `path:line:column
  (Construct)` locations while retaining generation-safe AST keys and exact
  source spans.

- Give the manifest-owned ABI-v5 soft builtin sole ownership of
  `__fiber_yield`, removing the retired Rust/JIT registration that caused
  duplicate-item failures throughout analysis.

- Keep the deny-warnings Rust gate compatible with current Clippy by using
  idiomatic `Option` propagation and direct mutable-builder reborrowing in
  generic-call queries and numeric operand lowering, and remove the obsolete
  runtime-intrinsic semantic-type fallback after its caller was retired.

- Restore common integer-width normalization for addition and subtraction so
  mixed `u8`/`i64` expressions emit verifier-clean CLIF.

- Align syntax-only codegen gate assertions with precise captured-expression
  diagnostics and the reachable zero-capture lambda body entry.

- Include the manifest-owned fiber-yield export and intrinsic in exact ABI-v5
  contract gate expectations.

- Keep the scheduler lifecycle gate aligned with the canonical core's explicit
  prohibition on emulating target context transfer.

- Publish rooted array growth replacements into a generation-proven mutable local or aggregate
  field before the owner barrier and exactly-once construction finish; canonical collection
  adapters no longer depend on assigning an unrooted `Array.Append` return value.

- Make canonical runtime arrays carry distinct array-object and element descriptors, scan their
  variable-size pointer payloads safely, root poll-owned pointers until deterministic release,
  drain channel handles during close/shutdown, cancel and join scheduler children at lifecycle
  teardown, and preserve representable alignment padding without false out-of-memory results.

- Attribute unsupported `if` conditions to the exact condition node by routing nested
  condition lowering through the diagnostic-preserving ISLE expression boundary.

- Retarget canonical Scheduler codegen coverage to the `Scheduler/Core.bd` owner introduced
  by the scheduler source split, preserving the facade as routing-only authority.

- Keep supplemental generic-specialization root discovery from rejecting an
  unrelated concrete function whose ABI facts are unavailable; selected
  executable items continue to fail closed in the authoritative source-item
  pass, and specialization failures now identify the exact call site.

- Restore `AbiParamKind`'s equality, hash, and copy derives after the manifest
  codegen module split, eliminating the stranded function-level attribute and
  preserving ABI parameter-array deduplication.

- Resolve calls through capture-free lambda local bindings from their
  generation-bound lexical initializer, preserving the syntax/ISLE closure
  path without materializing an unused runtime trampoline pointer.

- Keep the generated syntax-node inventory and SDK reflection kinds aligned with
  the CLIF block expression syntax surface.

- Resolve the Salsa semantic type of a nested direct call when generic
  specialization receives its enclosing expression node, preventing Corelib
  generic-call collection from failing on otherwise well-typed arguments.

- Derive native runtime provenance from the ABI-v5 manifest's complete target
  definition set. Core.Args service implementations and the selected native
  entry handoff are now accepted without widening the public export or loader
  requirement contracts; undeclared definitions still fail closed.

- Declare the `memcpy` and `strlen` dependencies emitted by the native
  Core.Args adapters in the target ABI-v5 import contract, so static runtime
  archive provenance remains exact rather than relying on compiler built-in
  lowering.

- Update the ABI-v5 platform-import contract witness for the declared math
  adapters and their target libraries, preserving exact import-set validation
  across Darwin, Linux, and Windows.

- Bring parse-recovery sync and grammar-rule keyword registries in line with
  the grammar's `clif`, `try`, and `catch` surfaces, restoring the
  grammar-completeness gate.

- Restore canonical runtime-kit syntax lowering by giving array annotations their
  ABI-v5 managed-pointer fact, using valid Beskid `i64` syscall result types,
  and resolving primitive conversion result facts from the conversion node
  rather than its input context. ISLE now reports the specific invariant when
  a proven numeric conversion cannot be emitted.

- Accept the manifest-owned Core.Args entry-adapter records when loading the
  generated ABI-v5 source and provenance audit contracts.

- Remove the stale local-initializer semantic-fact re-export and preserve the
  ABI-v5 `f64` mapping for intrinsic call signatures.

- Recognize the canonical ABI-v5 Windows target independently of the vendored
  `cargo-cross` target catalog, restoring COFF object emission for native
  runtime-kit staging.

- Keep the syntax formatter warning-free when rendering CLIF block expressions.

- Restore manifest-owned process adapter and Core.Math declarations, regenerating
  the ABI-v5 contract from the authoritative runtime manifest. Composite runtime
  allocations now retain and trace explicit managed child edges through GC.

- Restore `soft_builtin` as an ABI-v5 manifest-owned source construct for
  process-linked Core.Args services, and regenerate the checked-in Rust/JSON
  artifacts from that authority.

- Restore the Phase-B GC mutator transaction and opaque-allocation ownership
  contract after integration, while preserving the rooted typed-array allocator.
  Opaque payloads now remain construction-rooted until explicit publication, and
  mark/sweep phase transitions wait for active mutator work before reclamation.

- Reconcile the selected Corelib syntax-to-ISLE gate with the canonical test
  entrypoint names, so every configured probe now resolves to a maintained
  Corelib test before exercising lowering.
- Classify a generic call as a deferred template only when each explicit
  type argument is an actual generic parameter of its enclosing function.
  Concrete nominal calls such as `Channel<ConsoleMessage>.Create()` now
  materialize their direct syntax-ISLE specialization instead of requiring a
  nonexistent enclosing environment.

- Serialize the vendored `cargo-cross` tests that temporarily set
  `CARGO_PASSTHROUGH_ARGS`, preventing parallel tests from clearing each
  other's process-global fixture before parsing it.

- Normalize Darwin native-runtime provenance imports at the ABI policy boundary.
  The staged Mach-O adapter's canonical `exit` and `tlv_bootstrap` imports now
  match the manifest-derived allowlist, while undeclared imports remain rejected.

- Re-enable the multi-unit Corelib `Console.Controls.Frame.Repeat` JIT
  regression after repeated prepared-syntax → CodegenInput → engine execution
  proved the former missing-expression-type ordering failure is no longer
  reproducible.

- Replace the remaining ANSI corelib HIR link-plan probe with the canonical
  `SyntaxProgramAssembly` → `CodegenInput` → generated-ISLE route. The ANSI
  CSI bold-red regression now asserts the emitted ESC, CSI body, final byte,
  and expected-message literal bytes rather than relying on a diagnostic CLIF
  dump.

- Stage one explicit native ABI-v5 kit for the `beskid_tests` executable-linking
  helpers and pass its exact prefix to the installed-kit strategy. AOT execution
  tests no longer attempt to treat Cargo's `target/.../deps` test executable as
  an installed toolchain.

- Give every native ABI-v5 runtime-kit build its own staging directory. Parallel
  same-profile callers can no longer delete a peer build's static archive before
  it is published into the exact installed prefix.

- Rebind typed-array ISLE execution fixtures through concrete `JITModule`
  function and data imports before definition. The coverage now executes array
  indexing and indexed stores on macOS arm64 without relying on unsupported
  raw Cranelift test-case relocations.

- Serialize the ABI-v5 JIT integration tests' temporary installed-prefix
  contexts so the CodegenInput missing-manifest route deterministically reaches
  its intended exact-kit validation instead of racing another test's process
  environment restoration. Scheduler JIT execution now stages the same explicit
  native kit rather than attempting to derive a prefix from Cargo's test binary.

- Declare the canonical rooted typed-array allocation, construction-root release,
  and pointer write-barrier exports in the embedded runtime corpus so source
  authority remains complete for the ABI-v5 construction transaction.

- Contextually type unsuffixed integer literals only at explicitly typed local
  initializer and mutable-local assignment boundaries. Generated syntax ISLE
  now enforces the destination's exact ABI width and rejects variable-width
  assignment without an explicit primitive conversion, while inferred locals,
  explicit literal suffixes, compound values, immutable destinations, and
  out-of-range values fail closed.

- Lock the generated syntax-ISLE regression boundary against implicitly
  widening an `i32` variable during assignment to a mutable `i64` local.

- Restore type-safe semantic generic-specialization facts, preserving every
  `SyntaxGenerationId` bit in their structural identity, rejecting unprovable
  unused generic parameters, and collecting specializations reachable from
  executable test definitions.

- Resolve strict compiler lint failures in ABI-v5 array descriptor validation
  and the managed-heap helper implementation.

- Materialize generic syntax functions only from canonical direct-call ABI
  specialization facts. Uncalled generic declarations are omitted from module emission,
  while direct calls with absent or ambiguous specialization evidence now fail closed with
  their call and declaration identities.

- Lock local declarations and path/field/index assignments to their generated
  syntax → `CodegenInput` → ISLE dispatch rules, preventing canonical Core.String,
  input, and ANSI source from regressing to a missing-rule failure or a retired HIR path.

- Fail closed unless the ABI-v5 Core.Args manifest contains exactly
  `__args_count` and `__args_get`, and unless every target binding implements
  its canonical generated adapter symbol.

- Restrict `Core.Args` service authority to the compiler-owned regular file at
  its exact canonical path. Loader-proven materialized copies and symlink
  aliases now fail closed, while existing materialized Foundation services keep
  their established authorization behavior.

- Close Phase B GC publication races: opaque Beskid allocation has a must-use construction
  owner, and raw ABI returns retain that root until an external-root, handle, or insertion-barrier
  hand-off explicitly publishes the payload. Marking waits for Idle-admitted mutations before its
  root snapshot, and the admission/start transition uses one sequentially consistent ordering.
  Runtime arrays, strings, and dynamic cells now register the same canonical GC-visible composite
  edge before releasing an embedded allocation's construction root.
- Prevent concurrent Phase B collection from sweeping intrusive heap nodes while allocations or
  marking barriers are active. Mutator transactions now use an epoch-stamped phase admission
  handshake with RAII lifecycle cleanup, and sweep drains work published as admission closes.
- Preserve the canonical multi-unit `SchedulerSpawn` word-to-`i64` conversion
  fact through `CodegenInput` into generated ISLE coverage.
- Reject primitive numeric conversion facts whose semantic source or target
  differs from the source syntax before generated ISLE can emit CLIF.
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
- Reserve manifest-owned scheduler stacks as fixed 8 MiB ranges with inaccessible
  lower guards and 64 KiB initial writable suffixes across Linux, Darwin, and
  Windows. Add bounded in-place growth, exact whole-reservation release, scheduler
  committed/maximum accounting, and an explicit compiler stack-limit seam that
  reports denied growth or observed overflow as join status 3.

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
