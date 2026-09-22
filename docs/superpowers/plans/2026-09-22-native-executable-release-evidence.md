# Native executable release evidence implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove that the ABI-v5 native executable and Windows runtime artifacts conform to one manifest-owned lifecycle, argument, CRT, PE/COFF, and release-matrix contract.

**Architecture:** `executable_bootstrap.c` is the one process-lifetime owner for all native programs; generated Core.Args adapter provenance selects that same source when arguments are used. Runtime-kit construction remains the single Debug/Release evidence producer, while tests inspect actual linked PE/COFF artifacts rather than inferring them from linker flags or C drivers.

**Tech Stack:** Rust 2024, Cranelift, ABI-v5 BSOL manifest generation, C11 bootstrap, MSVC/lld-link, LLVM `llvm-readobj`/`llvm-nm`, native runtime-kit scripts.

**Spec:** No OpenSpec corpus is present in this compiler worktree. Binding authority is `AGENTS.md`, `runtime_manifest.bsol`, generated ABI-v5 artifacts, `scripts/stage-native-runtime-kit-matrix.sh`, and the tracked Windows native-pair tests.

## Global Constraints

- One executable lifecycle implementation: `crates/beskid_abi/assembly/common/executable_bootstrap.c`.
- Core.IO alone owns descriptor transfer loops, EOF, no-progress, and close semantics.
- Core.Args source selection must follow generated `entry_source` provenance; do not restore target-specific entry adapters.
- Windows executables leave startup selection to the CRT: no raw `/ENTRY:` linker argument.
- Runtime UCRT expectations derive from manifest imports with logical library `ucrt`; accept `ucrtbase.dll` and `api-ms-win-crt-*.dll` forwarders, reject `msvcrt.dll` for emitted applications.
- Every Windows C object participating in an emitted executable selects the dynamic CRT explicitly (`/MD`); link flags do not substitute for a consistent compile-time CRT selection.
- The host-only `beskid_program_main` linkage symbol is not a public export unless the user explicitly requested it through the artifact export policy.
- An executable's selected logical entry has one fixed host boundary (`beskid_program_main`): an incompatible explicit entry alias fails closed rather than being silently ignored. Both CRT startup spellings (`main`, `wmain`) are reserved for the host on every executable; explicit helper exports remain independent outside that reserved set.
- Bounded-process helpers supervise stdin transfer, stdout, stderr, deadline, and kill/reap as one concurrent lifecycle; no synchronous pipe write may precede that lifecycle.
- Use `llvm-readobj --coff-exports --coff-imports` for PE metadata and archive-aware `llvm-nm` for `.lib` symbol evidence.
- Keep the existing matrix script as the sole Debug/Release producer. Never fork its hashing or profile loop.
- Do not commit, push, merge, publish, mutate credentials, or change external services without explicit approval.

## Review Focus

- A non-`Main` executable entry must be remapped only at the executable host boundary and not leak an implicit public export.
- Unit-returning executables must shut down runtime state and return status zero.
- A Core.Args Windows app must use `wmain`, preserve argv-zero order, and import dynamic UCRT providers.
- The real runtime DLL must expose manifest loader exports and its import `.lib` must contain both direct and `__imp_` import-thunk symbols.
- A clean, exact compiler/Corelib closure must be required for the Debug/Release matrix; a dirty checkout must remain rejected.

---

### Task 1: Audit Windows PE imports and COFF import-library surface

**Files:**
- Modify: `crates/beskid_aot/tests/library_pair.rs`
- Test: `crates/beskid_aot/tests/library_pair.rs` (Windows x86_64 MSVC only)

**Interfaces:**
- Consumes: `AbiManifestV5::canonical_runtime(TargetMetadata)` and `RuntimeAuditMetadata::for_manifest`.
- Produces: test-only `read_windows_coff_imports(image) -> Vec<(String, String)>` and `read_windows_import_library_symbols(path) -> BTreeSet<String>` helpers.

- [x] **Step 1: Write failing PE/COFF assertions**

Add assertions to the existing two-profile platform-pair test which require each manifest `ucrt` import whose symbol begins with `_` to appear under a supported UCRT provider, and every manifest loader-required export to have both `<symbol>` and `__imp_<symbol>` in the paired import library.

- [ ] **Step 2: Run the Windows-only test to verify the new assertions fail before helpers exist**

Run with LLVM tools on PATH:

```powershell
cargo test --locked -p beskid_aot --test library_pair -- --exact windows_host_platform_pair_emits_a_coff_import_library_for_the_shared_runtime
```

Expected: a compile failure naming the missing test helpers, not an unrelated runtime or linker failure.

- [x] **Step 3: Implement minimal test-only artifact parsers**

Invoke `llvm-readobj --coff-imports <dll>` and parse each `Import { Name: ... Symbol: ... }` pair. Invoke `llvm-nm --extern-only <import.lib>` and retain `T <symbol>` records. Derive expected symbols from the manifest/audit metadata; do not add a handwritten runtime export list.

- [ ] **Step 4: Re-run the Windows test**

Run the Step 2 command with LLVM tools on PATH.

Expected: Debug and Release native runtime pairs pass with actual PE imports and COFF import-library symbols.

- [x] **Step 5: Record review evidence without committing**

Write the implementation report under the plan workspace, including the exact test command and whether every expectation derives from manifest metadata.

### Task 2: Validate the portable executable host across entry, lifetime, and CRT boundaries

**Files:**
- Modify only if the failing test requires it: `crates/beskid_abi/assembly/common/executable_bootstrap.c`, `crates/beskid_aot/src/api/object_stage.rs`, `crates/beskid_aot/src/api/platform_objects.rs`, `crates/beskid_aot/src/linker/windows.rs`
- Test: `crates/beskid_aot/src/object_module.rs`, `crates/beskid_engine/tests/foundation_io_native.rs`

**Interfaces:**
- Consumes: `EXECUTABLE_PROGRAM_ENTRY`, generated `GeneratedCoreArgsEntryAdapter`, ABI-v5 process init/shutdown exports.
- Produces: one `beskid_program_main` executable program boundary and an exact direct-application acceptance suite.

- [x] **Step 1: Add failing focused tests for reviewed host gaps**

Cover the reviewed defects before their corrections: assert Windows bootstrap compilation selects `/MD`; assert an unannotated non-`Main` executable entry is host-linkable but absent from public export metadata; and add bounded-harness cases for a full stdin pipe and an early child exit. Add direct emitted-app cases for a unit-returning non-`Main` entry, argv-zero/order/content including non-ASCII on Windows, and actual redirected input content. Align the macOS C adapter witness with the host's `int64_t` program return type. Do not test Core.IO loops in the C host.

- [ ] **Step 2: Run the focused tests on the matching native host and observe their failure**

Use the staged exact runtime kit and the target-specific ignored test selector. Expected: failure identifies an executable host/link boundary rather than a Core.IO implementation requirement.

- [x] **Step 3: Apply the smallest host, export, and harness corrections**

Keep lifecycle state aligned and zeroed, hand off Core.Args before `beskid_program_main`, shut down after a normal return, select `/MD` before Windows bootstrap compilation, and preserve CRT-selected startup without `/ENTRY:`. Keep required host linkage separate from returned public exports. Move stdin writing inside the existing concurrent bounded lifecycle; never add transfer policy to the bootstrap.

- [ ] **Step 4: Verify the direct application suite**

```powershell
cargo test --locked -p beskid_engine --test foundation_io_native staged_runtime_kit_emitted_binary_ -- --ignored
```

Expected: redirected input, output, error, combined streams, Core.Args handoff, and application UCRT provenance pass.

- [x] **Step 5: Record review evidence without committing**

Write the implementation report under the plan workspace, stating whether any lifecycle behavior changed and proving that Core.IO was not modified for host behavior.

### Task 3: Produce durable two-profile matrix evidence from a clean exact closure

**Files:**
- Test/operate: `scripts/stage-native-runtime-kit-matrix.sh`, `scripts/native-runtime-kit-evidence.py`, `scripts/aggregate-native-runtime-kit-evidence.py`
- Documentation: `CHANGELOG.md` only if the artifact test/matrix contract changes.

**Interfaces:**
- Consumes: a clean exact compiler/Corelib closure, target-native MSVC/LLVM environment, existing matrix script.
- Produces: hash-addressed Debug/Release runtime-kit evidence suitable for the aggregate verifier.

- [x] **Step 1: Verify dirty-source rejection**

Run `bash scripts/test-stage-native-runtime-kit-matrix.sh` from the dirty worktree.

Expected: fail closed because the compiler source is dirty; do not bypass or weaken that check.

- [ ] **Step 2: Obtain explicit approval for a local commit, then create a clean exact closure**

Do not proceed until approval permits the local commit. Preserve Corelib and compiler revisions together; do not push or merge.

- [ ] **Step 3: Run the sole matrix producer on the Windows native host**

Set `LLVM_READOBJ` to the installed LLVM tool and run the existing matrix script once. Retain its kit and evidence directory unchanged.

- [ ] **Step 4: Verify evidence with the existing verifier**

Run `native-runtime-kit-evidence.py` against the generated directory and inspect the two profile records. Use the aggregate verifier only once all three native target evidence directories exist.

- [ ] **Step 5: Record the exact revision, profile cells, hashes, and verifier result**

Store only non-secret evidence paths and hashes in the plan workspace. Do not publish or deploy artifacts.

### Task 4: Prove the complete compiler source closure before matrix production

**Files:**
- Modify: `scripts/native-runtime-kit-evidence.py`, `scripts/aggregate-native-runtime-kit-evidence.py`
- Test: focused evidence/aggregate regression coverage

**Interfaces:**
- Consumes: the compiler's actual enclosing-superproject relationship, root Gitlinks, compiler `corelib` Gitlink, and sibling `beskid_bsol` Gitlink.
- Produces: a full clean source-closure revision record in `producer.json` and an aggregate verifier that requires that record to agree across targets.

- [x] **Step 1: Add focused failing closure tests**

Create clean temporary Git fixtures that prove a valid root/compiler/Corelib/BSOL closure, then independently break the compiler relationship, a Gitlink, or any clean status. Each invalid closure must fail before artifact production.

- [x] **Step 2: Resolve and verify the actual source closure in the existing initializer**

Discover the enclosing root through Git's submodule relationship rather than a parent-directory guess. Require root direct sources and selected compiler/Corelib/BSOL sources clean; require every selected child `HEAD` to match its parent Gitlink; write every revision and clean fact to the existing producer receipt. Unrelated root submodules are immutable Gitlink placeholders outside the selected build closure. A detached compiler worktree without a provable enclosing Gitlink must fail closed.

- [x] **Step 3: Extend aggregate closure validation**

Reject evidence that omits, dirties, or disagrees on any closure revision. Bump schema compatibility deliberately if required; never accept a legacy partial receipt as a durable full-closure proof.

- [x] **Step 4: Run focused local regression tests and record evidence**

The test must prove valid closure acceptance and each fail-closed path. Record commands/results without committing.

### Task 5: Seal and replay the complete source closure locally

**Files:** `scripts/native-source-seal.py`, `scripts/native_runtime_source_closure.py`, `scripts/test-native-source-seal.py`.

- [x] Add disposable-Git acceptance tests before implementing the local transport.
- [x] Reuse the existing closure authority for committed Gitlinks and repository discovery; retain clean defaults for matrix consumers.
- [x] Package each repository's original commit objects in a Git bundle, plus canonical SHA-256 receipt and explicitly allowlisted binary/untracked diagnostic overlays.
- [x] Verify/replay into a fresh temporary checkout before publishing a destination; reject digests, paths, symlinks, malformed inventories, Gitlink disagreement, or independent consumer allowlist disagreement.
- [x] Reject `diagnostic_dirty` when the consumer purpose is `matrix`; preserve the existing matrix as sole producer.
- [x] Harden the shared Git runner against ambient repository/config/filter state; reject Windows-reserved and NFC/casefold-colliding paths; atomically publish only verified packages.
- [x] Preserve unrelated root Gitlinks as uninitialized placeholders while rejecting undeclared nested selected-repository links and overlays crossing any Gitlink; use the same selected-source definition in evidence v3.
- [x] Enumerate every approved diagnostic path directly; reject missing/unused approvals and clean-mode overlays. Ignore rules cannot conceal input; only the reserved `compiler/target/` output tree is excluded consistently by sealing and matrix closure discovery.
- [x] Independently review the seal implementation and its local replay acceptance suite.
- [ ] Exercise it on the actual target platform before operational transport.

The receipt digest and consumer allowlist must come from a trusted channel independent of the package. This task creates no source medium, transfers no archive, and does not start a builder. Cargo metadata/path-dependency resolution remains an executor preflight before running native tests.

## Self-review

- Task 1 covers actual runtime DLL providers and COFF import-library thunks, not only command shape.
- Task 2 covers the direct emitted app independently of the hosted C Foundation fixture and preserves Core.IO ownership.
- Task 3 keeps one matrix producer and refuses unverifiable dirty-source evidence.
- No task creates a fallback startup, duplicate argument bridge, duplicate I/O loop, hand-maintained export list, or second profile protocol.
