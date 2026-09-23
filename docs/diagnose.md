# Diagnose playbook

`scripts/diagnose/` holds read-only diagnostic tools for the failures that show up repeatedly
while running compiler/corelib tests on a remote builder. They exist so a diagnosis does not have
to be re-derived by hand every time the same error class recurs. None of them build or modify
anything; several take an explicit `--rerun` or print (never run) a repair command instead of
running it.

All tools accept the builder host, jump host, container name, repo root, and workspace/target-dir
layout as environment variables (or flags where noted), documented per tool below. Defaults match
the builder this playbook was written against; override them for a different builder.

Run everything from the repo root, e.g.:

```
scripts/diagnose/status.sh
scripts/diagnose/kitcheck.sh <slice>
echo '...log text...' | scripts/diagnose/symbolize.py
```

## Configuration reference

| Env var | Meaning | Default |
|---|---|---|
| `BESKID_DIAG_JUMP_HOST` | ssh `-J` jump host | `root@bdziam.dev` |
| `BESKID_DIAG_BUILDER_HOST` | ssh target host, reached via the jump host | `root@10.66.0.2` |
| `BESKID_DIAG_CONTAINER` | podman container name on the builder | `beskid-codex-build` |
| `BESKID_DIAG_REPO_ROOT` | local repo root for source cross-references | this script's own repo (two dirs above `scripts/diagnose/`) |
| `BESKID_DIAG_WORKSPACE` | builder-side checkout parent (`<workspace>/compiler-<slice>`, `<workspace>/verify`) | `/workspace` |
| `BESKID_DIAG_TARGET_DIR` | builder-side cargo target root (`<target-dir>/<slice>/debug/...`) | `/target` |
| `BESKID_DIAG_LOG_DIR` | builder-side per-slice log directory | `/var/lib/beskid-codex-build/run` |

The Python tools (`symbolize.py`, `whyfact.py`, `authority.py`, `visibility.py`) only need
`BESKID_DIAG_REPO_ROOT` (or `--repo`); they never touch the builder.

## Tool index

| Tool | Purpose | Needs builder access |
|---|---|---|
| `symbolize.py` | Expand a `path#gN:nM[ Construct@span]` diagnostic key into a real file + source snippet | no |
| `whyfact.py` | Trace a `MissingRuleOrFact` / "semantic query unavailable" error to its Rust source and one-hop caller | no |
| `authority.py` | Explain a "canonical X unavailable" error (two distinct mechanisms) | no |
| `visibility.py` | Explain an "unknown value `X`" error when X clearly exists in source | no |
| `crashdiag.sh` (`crashinfo.sh` is a compatibility wrapper) | Diagnose a fatal-signal failure: signal meaning, coredump backtrace, trap classification, OOM check | yes |
| `kitcheck.sh` | Explain a `SourceHashMismatch`: recompute the kit's source hash and list files newer than the kit | yes |
| `envcheck.sh` | Preflight a slice's test environment (corelib fingerprint, kit, binary, cache) before blaming product code | yes |
| `clifdiff.sh` | Dump and diff normalized CLIF between two slices/inputs, or two saved dumps | yes (unless `--files`) |
| `status.sh` | One-shot snapshot: running processes, load average, newest log per slice | yes |
| `stalled.sh` | Flag a log that looks in-progress but has no writer left | yes |

## Error class -> tool -> usual fix

### `MissingRuleOrFact`

Emitted by the ISLE lowering path when a `NodeFacts` query needed to select an emitter has no
answer for the given AST node. The diagnostic carries a `path#gN:nM Construct@span` key.

1. `symbolize.py` on the diagnostic text to get the real file and source snippet.
2. If the message also names a `semantic query` (see below), run `whyfact.py` on that query name.
3. Usual fix: the ISLE rule set is missing a rule for this AST node shape, or the `NodeFacts`
   implementation doesn't derive the fact this construct needs. Check
   `docs/isle-lowering-coverage.md` for whether the construct is a known gap.

### `semantic query \`X\` is unavailable`

1. `whyfact.py X` (or pipe the log into it; it extracts the query name itself). It prints:
   - the Rust function(s) whose `SemanticError::unavailable("X")` call fired, and where,
   - their one-hop callers under `beskid_isle`/`beskid_codegen`/`beskid_queries`.
2. One query name can have several failure sites in the same function (different branches) --
   `whyfact.py` lists all of them, not just the first.
3. Usual fix: either the semantic_contract implementation for `X` needs a case for this AST shape,
   or (more often) the ISLE/codegen caller is requesting the query at a point where the fact
   genuinely isn't derivable yet (wrong lowering order) -- the one-hop caller list is the starting
   point for that judgment call, not an automatic answer.

### `unknown value \`X\`` when X exists in source

This is drift diagnosis (module path / `pub` / import mismatch), not a resolver bug by default.

1. `visibility.py X --project <path to the failing .bproj or its src/>`.
2. It reports every declaration site with its inferred module path and `pub`/private status, and
   (with `--project`) every reference to `X` in that project plus its own `use` lines.
3. Read the printed verdict:
   - not `pub` anywhere -> add `pub` at the declaration.
   - `pub`, but no consuming file directly `use`s the declaring module (it only imports a facade
     that `pub mod`-re-exports it) -> likely import drift; try adding the direct `use` first.
   - `pub` and directly `use`d, but still unknown -> the tool says explicitly this is unexplained
     from source text alone; escalate as a possible resolver bug rather than assuming more drift.

### `canonical X ... unavailable`

Two distinct mechanisms produce this exact message shape; `authority.py X` checks both and
reports which applies:

- **Mechanism A -- raw-builtin corelib-service authority**
  (`crates/beskid_abi/src/runtime_source/corelib_services.rs`, `CORELIB_SERVICES` table): a raw
  call like `__timer_sleep_until` is authorized only from the specific source file(s) listed for
  it. Usual fix: either call it from an authorized file, or add the calling file's
  `CANONICAL_..._SOURCE_PATH` constant to the table entry.
- **Mechanism B -- scheduler-entry reachability**
  (`crates/beskid_codegen/src/module_emission.rs` `SCHEDULER_ENTRY_HELPERS`, checked in
  `module_emission/orchestration.rs`): once any scheduler entry/return trampoline is required,
  *all* of the named items must be reachable from the compiled module's entry point. Usual fix:
  the compiled project doesn't transitively `use` the module declaring the missing item -- cross
  check with `visibility.py`.

`authority.py` prints the exact table entry or reachability list and every real call/reference
site so you can see directly which file is calling vs. which file(s) are authorized, rather than
guessing from the error text alone.

### `SourceHashMismatch`

The runtime kit's embedded `abi.json` records a `source_hash` computed over every mapped
runtime/corelib `.bd` file (plus a generated ABI-layout prefix for `AbiValue.bd`, tracked
separately). A mismatch means the kit was built from a different source state than the one the
compiler now embeds.

1. `kitcheck.sh <slice> [kit-dir-name]` (default kit dir: `network-kit.<slice>`).
2. It recomputes the hash on the builder over the *current* slice checkout and reports:
   - exact match/mismatch on the non-`AbiValue.bd` subset,
   - any mapped file missing on the remote checkout (manifest drift between `sources.rs` and the
     slice tree -- a different kind of bug, worth noting separately),
   - every mapped file whose mtime is newer than the kit's `abi.json` -- the prime suspects.
3. Usual fix: rebuild the kit (the tool prints the exact `runtime-kit build-native-host` command
   with the kit's own prefix and profile). If no file is newer than the kit but the mismatch
   persists, the kit was built from a different worktree/commit, or `beskid_cli` itself embeds a
   different corpus -- rebuild `beskid_cli` first, then the kit.

### `UnprovenCollectionOwner`

Not yet covered by a dedicated tool. In practice this has traced back to the same two classes as
`MissingRuleOrFact`/`canonical X unavailable`: either a missing ownership fact in the ISLE
`NodeFacts` path (use `symbolize.py` + `whyfact.py` the same way, substituting the collection-owner
query name), or a reachability gap the compiled project doesn't transitively `use` (cross-check
with `visibility.py` on the owner symbol). Treat it as a `MissingRuleOrFact` variant for triage
purposes until a dedicated check is written.

### The shared-corelib-root race (`BESKID_CORELIB_ROOT` per slice)

Symptom: `fingerprint installed corelib at $HOME/.beskid/beskid_corelib: No such file or
directory`, intermittent and not reproducible from source alone.

Cause: every slice's `beskid_cli`, unless installed under a proper `<prefix>/bin` layout, falls
back to the *same* `$HOME/.beskid/beskid_corelib` path inside the shared build container. When
more than one slice's `beskid_cli` runs at once, one process's remove-then-recreate races another
process's directory walk.

1. `envcheck.sh <slice>` -- preflights the corelib fingerprint path, the runtime kit (via
   `kitcheck.sh`), the built binary, and the `obj/beskid` cache in one pass, and lists concurrent
   `beskid_cli` processes.
2. If checks 2-4 are clean and the fingerprint error recurs, it is almost certainly this race, not
   a product bug. Fix by giving the slice its own `BESKID_CORELIB_ROOT` (the tool prints the exact
   command) rather than sharing the container-wide default path.

### SIGILL / SIGSEGV / exit 137 and other fatal signals

`crashdiag.sh` (or the older call shape via `crashinfo.sh <target> [slice] [--no-rerun]`) turns an
exit/signal code into: what it usually means in this codebase, the matching coredump backtrace
(symbolized where possible, JIT frames reported as expected-unsymbolizable rather than as an
error), and for SIGILL a best-guess trap classification cross-referenced against
`crates/beskid_isle/src/context/**` trap-emission sites. It also checks `dmesg`/`journalctl` for
direct kernel corroboration, which is what separates a real SIGKILL-from-OOM from a
misattributed product bug.

- **SIGILL (132)**: historically a deliberate Cranelift `trap`/`trapz`/`trapnz` (bounds/null/GC-
  barrier/overflow check) lowered to `ud2`, not stack overflow, in this codebase. Confirm the trap
  kind from the dmesg corroboration and trap-emission-site list the tool prints; it does not
  invent a trap kind it cannot evidence.
- **SIGSEGV (139)**: consistent with this codebase's stack-guard-page mechanism if the crash site
  is guard-related (a guard-page hit can have a shallow, otherwise-unremarkable frame count), or a
  bad/dangling pointer elsewhere. The tool states which without asserting one from frame count
  alone.
- **exit 137 (SIGKILL)**: almost always the OOM killer. Always check the OOM evidence line the
  tool prints (`journalctl -k`) before attributing this to product logic.
- Every other code in the 128+signal table (SIGABRT 134, SIGTRAP 133, SIGFPE 136, SIGBUS 138) is
  covered in `crashdiag.sh`'s own header comment with the same evidence-first treatment.

Usage:
```
scripts/diagnose/crashdiag.sh --target TextRegexTests --slice cov
scripts/diagnose/crashdiag.sh --log /path/to/captured.log
scripts/diagnose/crashdiag.sh --exit 132 --bin /target/cov/debug/beskid_cli
```

### Structural codegen regressions (CLIF-level)

`clifdiff.sh` dumps CLIF from two slices/inputs (or diffs two already-saved dumps with `--files`)
after normalizing away cosmetic renumbering (`v<N>`, `block<N>`, `sig<N>`, `fn<N>`, and Beskid's
`#syntax_<file>_<node>` label suffix) so only a real structural difference is left. Use this to
confirm whether a suspected regression actually changed the generated IR, or is just numbering
noise between two independently compiled dumps.

## Builder etiquette

- **One cargo/beskid_cli process per slice at a time.** Concurrent invocations from the same
  slice race the same output paths and logs; `status.sh` and `stalled.sh` exist specifically to
  catch when this has gone wrong (a stale-looking dead run, or two writers to one log).
- **~4 slices max concurrently on the shared builder.** Beyond that, load average and the shared
  `$HOME/.beskid/beskid_corelib` fallback path (see the corelib-root race above) both degrade;
  give each concurrent slice its own `BESKID_CORELIB_ROOT` if you must exceed this informally.
- **Check `status.sh` before starting new work** -- it is one ssh round trip and shows every
  slice's newest log, its age, and its last result/EXIT line, so you don't duplicate a run that's
  already in flight or already finished.
- **Use `stalled.sh [N_MIN]`** (default 20 minutes) to find a log that looks in-progress but has
  no process left writing it, before assuming a run is still working.
- These tools are read-only by design (`envcheck.sh`, `kitcheck.sh`, `crashdiag.sh` only print
  repair/rebuild commands, they never run them, except `crashdiag.sh --target`'s optional
  `--rerun`, which reruns only the one failing target under trace and is on by default -- pass
  `--no-rerun` to suppress it).
