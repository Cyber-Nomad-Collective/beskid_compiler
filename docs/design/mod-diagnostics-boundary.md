# Mod Diagnostics Boundary — End-to-End Design

Status: **Design (no implementation)**. Scope: full compiler↔Beskid mod diagnostics
boundary — structured diagnostics with spans, quick-fixes (edits), LSP code-action
surfacing, and build-failure on mod Errors. No stubs.

Every claim below is verified against the tree at HEAD and cites `file:line`.

---

## 0. Verified gap table (G1–G9)

| Gap | Claim | Evidence | Verdict |
|-----|-------|----------|---------|
| G1 | `AnalysisResult {}` empty; `AnalysisRequest` carries only `CollectRequest`; `Analyzer` returns nothing; `FixError {}` empty | `compiler/corelib/packages/compiler-sdk/src/Beskid/Compiler/Collect.bd:24-26,28,33,77-79` | CONFIRMED |
| G2 | `ModAnalysisRequest { context }` — no snapshot; `ModAnalysisResult { diagnostics }` — no fixes; `ModDiagnostic` has no quick-fix field | `compiler/crates/beskid_abi/src/mod_contract.rs:127-130,185-196,207-210` | CONFIRMED |
| G3 | `native.rs:151` builds `ModAnalysisRequest { context: *request }` and drops `snapshot`; `:166` hardwires `fix_targets: Vec::new()` | `compiler/crates/beskid_analysis/src/mod_host/native.rs:147-151,164-166` | CONFIRMED |
| G4 | `AnalyzerDiagnostic { code, message, severity, span }` — no quick-fix field | `compiler/crates/beskid_analysis/src/mod_host/invoker.rs:57-66` | CONFIRMED |
| G5 | `SemanticSnapshot` is fingerprints + counts only — no types/symbols/scopes | `compiler/crates/beskid_analysis/src/services/session.rs:49-61` | CONFIRMED |
| G6 | `Query.bd` is syntax-only — no `Beskid.Compiler.Semantic` module | `compiler/corelib/packages/compiler-sdk/src/Beskid/Compiler/Query.bd:1-146` (only `Syntax*` types + `As*` node casts) | CONFIRMED |
| G7 | Mod diagnostics surface ONLY on `collect_diagnostics` branch; typed/codegen path discards `analyzer_outcomes` | `compiler/crates/beskid_analysis/src/services/prepare.rs:286-306` (loop guarded by `if collect_diagnostics` at `:295`); `:108-130` calls spine with `collect_diagnostics=false` (`:126`); only `mod_rewrite.program` consumed (`:293`) | CONFIRMED |
| G8 | LSP `code_actions/handler.rs` is a closed match on hardcoded codes; no mod-supplied registry | `compiler/crates/beskid_lsp/src/features/code_actions/handler.rs:47-113` (closed `match` on `W1503`/`W1639`/`W1610`-`W1625` at `:64-97`); `CodeActionKind::QUICKFIX` at `:39,124,140` | CONFIRMED |
| G9 | No `QuickFix`/`Suggestion`/`CodeAction` type in `beskid_abi`/`beskid_analysis`; `Diagnostics.bd` has only `SemanticIssueKind` + `Severity` — no `Diagnostic` struct, no emit helper | `compiler/corelib/packages/compiler-sdk/src/Beskid/Compiler/Diagnostics.bd:5-149` (only the two enums + a version fn at `:155-157`) | CONFIRMED |

Additional facts established during verification (used by the design):

- `ModEdit` already exists and is the exact shape needed for text fixes:
  `compiler/crates/beskid_abi/src/mod_contract.rs:213-224` (`kind: u32` 0=Insert/1=Replace/2=Delete, `start`, `end`, `text: BeskidStr`). It is currently used only by `ModRewriteResult` (`:233-237`).
- `RewriteEdit` is the host-side mirror of `ModEdit`:
  `compiler/crates/beskid_analysis/src/mod_host/invoker.rs:78-86` (`Insert { offset, text }` / `Replace { start, end, text }` / `Delete { start, end }`).
- `AnalyzerOutcome { type_id, diagnostics, fix_targets: Vec<String> }`:
  `compiler/crates/beskid_analysis/src/mod_host/invoker.rs:49-54`. `fix_targets` is a vestigial string list, never populated by the native invoker (`native.rs:166` hardwires `Vec::new()`).
- `ModHostAnalyzeResult { program, analyzer_outcomes, rewriter_outcomes, edited_source }`:
  `compiler/crates/beskid_analysis/src/mod_host/types.rs:138-149`.
- `analyzer_diagnostic_to_semantic` maps one `AnalyzerDiagnostic` → one `SemanticDiagnostic`, encoding the mod contract id only in `help` (`format!("mod analyzer contract `{type_id}`")`):
  `compiler/crates/beskid_analysis/src/mod_host/diagnostics.rs:151-175`. There is **no origin/source field** on `SemanticDiagnostic` (`compiler/crates/beskid_analysis/src/analysis/diagnostics.rs:22-33`).
- `SemanticDiagnostic` struct literals exist in **6 files / 15 sites** (blast radius for adding a field): `analysis/diagnostics.rs` (5), `services/diagnostics_emit.rs` (4), `mod_host/diagnostics.rs` (2), `macros/diagnostics.rs` (2), `services/composition.rs` (1), `format/emit.rs` (1).
- LSP `SyntaxDiagnostic { start, end, severity, code, message }` has **no source/origin field**:
  `compiler/crates/beskid_lsp/src/session/store.rs:54-60`. `syntax_to_lsp_diagnostic` hardcodes `source: Some("beskid".to_string())`:
  `compiler/crates/beskid_lsp/src/diagnostics.rs:138-151` (`:147`).
- LSP `code_action` dispatch is a single function call with no provider registry:
  `compiler/crates/beskid_lsp/src/server/backend.rs:262-268`; capability declared as `Simple(true)` at `server/init.rs:29`.
- `ensure_snapshot_for_analyzer` requires the snapshot to satisfy `composition` stage but only checks `staged_through` rank — the snapshot carries no data the mod can read:
  `compiler/crates/beskid_analysis/src/mod_host/analyze.rs:40-64`.
- `ModInvocationContext::compilation` interns `entry_source_path` and `entry_source_name` from `input.source_name` but **never interns `input.source`** (the text):
  `compiler/crates/beskid_analysis/src/mod_host/context.rs:77-90`. `ModCompilation` has no source-text field: `compiler/crates/beskid_abi/src/mod_contract.rs:19-28`.
- SDK surfaces are regenerated by `regen_mod_sdk_surfaces.sh`. `Collect.bd` and `Query.bd` are **hand-maintained facades** (the script emits them verbatim from heredocs — `regen_mod_sdk_surfaces.sh:203-285` for `write_collect`, `:34-46` for `write_query`). `Diagnostics.bd` is **reflect-generated** for `Severity` + `SemanticIssueKind` then appends a hand-maintained facade (`:48-66`). Reflect generation is driven by `#[beskid_ast_derive::beskid_reflect]` enums in `compiler/crates/beskid_analysis/src/compiler_sdk_reflect.rs:101-128`.

---

## 1. ABI changes — `compiler/crates/beskid_abi/src/mod_contract.rs`

### 1.1 Quick-fix shape on `ModAnalysisResult`

**Decision: flat fixes list with `diagnostic_index` link** (not one-fix-per-diagnostic).

Rationale:
- A diagnostic may have zero fixes (common) or many. Embedding an edits slice in
  every `ModDiagnostic` wastes the ABI for the zero-fix majority and caps a
  diagnostic at one fix.
- LSP `CodeAction.diagnostics` is a `Vec<Diagnostic>` — an action references the
  diagnostics it fixes, not the reverse. The flat-list-with-index mirrors that.
- `diagnostic_index` is a `u32` into the same `ModAnalysisResult.diagnostics`
  slice, so the host can resolve the linked diagnostic without string matching
  (codes can collide across mods).

New structs (insert after `ModDiagnosticSlice` at `mod_contract.rs:198-203`):

```
// Reuses ModEdit (mod_contract.rs:213-224) for the edit payload.
#[repr(C)]
pub struct ModQuickFix {
    pub diagnostic_index: u32,   // indexes into ModAnalysisResult.diagnostics
    pub title: BeskidStr,
    pub edits: ModEditSlice,      // reuses ModEditSlice (mod_contract.rs:226-231)
}

#[repr(C)]
pub struct ModQuickFixSlice {
    pub items: *const ModQuickFix,
    pub len: usize,
}
```

Widen `ModAnalysisResult` (`mod_contract.rs:207-210`):

```
#[repr(C)]
pub struct ModAnalysisResult {
    pub diagnostics: ModDiagnosticSlice,
    pub fixes: ModQuickFixSlice,   // NEW
}
```

`ModEdit` (`mod_contract.rs:213-224`) is reused unchanged — its `kind`/`start`/`end`/`text`
is exactly the Insert/Replace/Delete shape a quick-fix needs. No new edit type.

### 1.2 Snapshot decision — Option C now, Option A documented as Phase 2

**Decision: defer semantic exposure (Option C).** Forward a `ModSemanticHandle`
that is `null` in this phase; mods re-derive from source text + syntax tree.

Options considered:

- **Option A — `Beskid.Compiler.Semantic` query callback table.** The ABI request
  carries a vtable of function pointers (e.g. `resolve_type(path) -> TypeId`,
  `type_fields(type_id) -> FieldSlice`, `expected_type_at(node_ref) -> TypeId`).
  The host implements the vtable by routing to `beskid_queries` (the Salsa
  semantic authority — `compiler/crates/beskid_queries`). This is the right
  long-term shape: lazy, version-resilient, consistent with the "Salsa is the
  semantic authority" rule. **Cost:** a stable callback ABI, host-side query
  dispatch, a new `Beskid.Compiler.Semantic` SDK module, and a
  `ReflectSdkSemanticQueryPlane` reflect enum. Large, separable effort.
- **Option B — snapshot carries serialized type/field data.** Embed a serialized
  blob (type table + field sets) in the ABI request. **Rejected:** one-shot
  transfer of a large payload, brittle versioning (snapshot schema must match
  mod SDK schema exactly), and the mod must deserialize. Conflicts with the
  incremental Salsa model.
- **Option C — defer; mods re-derive from syntax.** Mods get source text +
  syntax tree (via the existing `Query.bd` surface, which is syntax-only — G6)
  and emit diagnostics from syntactic patterns. **Chosen for this phase.**
  Ships the full diagnostics + quick-fix boundary now; "missing fields"
  diagnostics that need resolved struct types wait for Option A.

Rationale for C-now:
1. The current `SemanticSnapshot` is fingerprints + counts only (G5,
   `session.rs:49-61`). Forwarding it across the ABI is useless — the native
   invoker already drops it (`native.rs:147-151`) and the stub only records
   `version`/`staged_through` (`invoker.rs:246-260`).
2. `ensure_snapshot_for_analyzer` (`analyze.rs:40-64`) gates on `staged_through`
   rank but the snapshot carries no queryable data — so the gate is purely a
   "composition happened" latch, not a data channel.
3. Building the Option-A callback table is a large, independently shippable
   effort that should not block the diagnostics + quick-fix boundary.

**Forward-compatible ABI shape** (so Phase 2 does not break the struct layout):

```
#[repr(C)]
pub struct ModSemanticHandle {
    pub ptr: *const c_void,   // null in Phase 1 (Option C); callback vtable in Phase 2 (Option A)
    pub version: u32,         // 0 = no semantic surface; 1 = vtable v1
}
```

Widen `ModAnalysisRequest` (`mod_contract.rs:127-130`):

```
#[repr(C)]
pub struct ModAnalysisRequest {
    pub context: ModCollectRequest,
    pub semantic: ModSemanticHandle,   // NEW — null.ptr in Phase 1
}
```

The host sets `semantic: ModSemanticHandle { ptr: null(), version: 0 }` in
Phase 1. The mod checks `if semantic.ptr.is_null()` and falls back to
syntax-only. In Phase 2 the host populates `ptr` with a vtable and bumps
`version`; the struct layout is unchanged, so native artifacts compiled against
Phase 1 keep working (they see `version: 0` and ignore `ptr`).

### 1.3 Source-text decision — add `entry_source_text` to `ModCompilation`

**Decision: add `entry_source_text: BeskidStr` to `ModCompilation`.**

Today the mod reads `entry_source_path` from disk (`mod_contract.rs:26`). The
host already holds the source text — `ModHostInput.source: &'a str`
(`types.rs:111`) — but `ModInvocationContext::compilation` (`context.rs:77-90`)
interns `source_name` twice (as `entry_source_path` and `entry_source_name`,
`context.rs:87-88`) and never interns `source`. So mods are forced to do disk
I/O for text the host already has in memory.

Widen `ModCompilation` (`mod_contract.rs:19-28`):

```
#[repr(C)]
pub struct ModCompilation {
    pub active_project_name: BeskidStr,
    pub active_project_root: BeskidStr,
    pub target_triple: BeskidStr,
    pub syntax_generation_id: u64,
    pub entry_source_path: BeskidStr,
    pub entry_source_name: BeskidStr,
    pub entry_source_text: BeskidStr,   // NEW
}
```

`ContextArena::compilation` (`context.rs:77-90`) adds
`entry_source_text: self.intern(input.source)`. The empty-context test helper
`empty_collect_request_for_test` (`native.rs:360-380`) and `ModInvocationContext::empty`
(`context.rs:47-59`) add an empty `entry_source_text`.

### 1.4 Touch summary for Layer 1

| File:line | Change |
|-----------|--------|
| `beskid_abi/src/mod_contract.rs:19-28` | add `entry_source_text: BeskidStr` to `ModCompilation` |
| `beskid_abi/src/mod_contract.rs:127-130` | add `semantic: ModSemanticHandle` to `ModAnalysisRequest` |
| `beskid_abi/src/mod_contract.rs:198-203` (after) | insert `ModQuickFix` + `ModQuickFixSlice` |
| `beskid_abi/src/mod_contract.rs:207-210` | add `fixes: ModQuickFixSlice` to `ModAnalysisResult` |
| `beskid_abi/src/mod_contract.rs` (new) | `ModSemanticHandle` struct |

**Impact:** `ModCompilation` and `ModAnalysisRequest` are `#[repr(C)]` and
copied by value across the ABI. Adding a field changes the struct layout, so
native mod artifacts must be recompiled against the new SDK. This is expected
(the ABI is pre-1.0; `CollectFacadeVersion` is `0.4.0` at `Collect.bd:92`). Bump
`CollectFacadeVersion` and `ModSdkCompilationSurfaceVersion`
(`regen_mod_sdk_surfaces.sh:281,321`) to signal the break.

---

## 2. Host changes — `compiler/crates/beskid_analysis/src/mod_host/` + `services/prepare.rs`

### 2.1 `invoker.rs` — widen `AnalyzerOutcome` and add `AnalyzerFix`

Replace the vestigial `fix_targets: Vec<String>` (`invoker.rs:53`) with a
structured fixes list. Reuse `RewriteEdit` (`invoker.rs:78-86`) for the edit
payload — it is already the host mirror of `ModEdit`.

`invoker.rs:49-54` becomes:

```
pub struct AnalyzerOutcome {
    pub type_id: String,
    pub diagnostics: Vec<AnalyzerDiagnostic>,
    pub fixes: Vec<AnalyzerFix>,   // REPLACES fix_targets: Vec<String>
}

pub struct AnalyzerFix {
    pub diagnostic_index: u32,   // indexes into AnalyzerOutcome.diagnostics
    pub title: String,
    pub edits: Vec<RewriteEdit>,
}
```

`AnalyzerDiagnostic` (`invoker.rs:57-66`) is **unchanged** — the fix carries the
link, not the diagnostic. `AnalyzerSeverity` (`invoker.rs:68-74`) unchanged.

`StubContractInvoker::invoke_analyzer` (`invoker.rs:246-260`) and
`ScriptedContractInvoker::invoke_analyzer` (`invoker.rs:402-416`) update from
`fix_targets` to `fixes` (default empty; scripted overlay adds a
`with_analyzer_fix` helper mirroring `with_analyzer_diagnostic` at
`:345-348`). The `scripted_overlays_analyzer_diagnostics` test (`:479-497`)
and `analyzer_diagnostic_defaults_to_warning_severity_and_no_span` test
(`:517-523`) update.

### 2.2 `native.rs` — forward source text, unmarshal fixes, stop dropping snapshot

`native.rs:143-170` (`invoke_analyzer`):

- `:151` — `ModAnalysisRequest { context: *request }` becomes
  `ModAnalysisRequest { context: *request, semantic: ModSemanticHandle { ptr: null(), version: 0 } }`.
  (Phase 1: no semantic surface. The `snapshot: Option<&SemanticSnapshot>` Rust
  param is still accepted by the trait but, as established in G5, carries no
  queryable data — so it is not forwarded as data. It remains the
  `ensure_snapshot_for_analyzer` gate (`analyze.rs:40-64`).)
- `:164-166` — unmarshal fixes from `result.fixes` and populate
  `AnalyzerOutcome { type_id, diagnostics, fixes }` instead of
  `fix_targets: Vec::new()`.

New unmarshal helper (next to `unmarshal_diagnostics` at `native.rs:261-291`):

```
fn unmarshal_fixes(slice: &ModQuickFixSlice, diagnostics_len: usize) -> Vec<AnalyzerFix> {
    // bounds-check diagnostic_index against diagnostics_len; drop out-of-range fixes
    // (fail-closed: a mod that emits a bad index loses the fix, not the build)
}
```

`unmarshal_edits` (`native.rs:294-312`) is reused unchanged — `ModQuickFix.edits`
is a `ModEditSlice`, the same shape `unmarshal_edits` already consumes.

The `entry_source_text` interning happens in `context.rs` (Layer 2.5 below),
not here.

### 2.3 `types.rs` — surface fixes on `ModHostAnalyzeResult`

`ModHostAnalyzeResult` (`types.rs:138-149`) already exposes
`analyzer_outcomes: Vec<AnalyzerOutcome>` (`:141`). Because `AnalyzerOutcome`
now carries `fixes`, no field change is needed on `ModHostAnalyzeResult` — the
fixes ride inside `analyzer_outcomes[].fixes`. The prepare spine and LSP read
them through `mod_rewrite.analyzer_outcomes`.

### 2.4 `diagnostics.rs` — add fix mapping and origin tag

Two changes to `compiler/crates/beskid_analysis/src/mod_host/diagnostics.rs`:

1. **Origin tag.** `analyzer_diagnostic_to_semantic` (`:151-175`) must tag the
   diagnostic as mod-origin so LSP can route code actions. Today the mod
   contract id is encoded only in `help` (`:172`), which is not a routing
   channel. Add an `origin` field to `SemanticDiagnostic` (see 2.6) and set it
   here to `Some(format!("beskid:mod:{type_id}"))`.

2. **Fix mapping.** Add a sibling helper:

```
pub fn analyzer_fix_to_syntax_fix(
    fix: &AnalyzerFix,
    outcome: &AnalyzerOutcome,
    source: &str,
) -> SyntaxFix { ... }
```

This maps `AnalyzerFix { diagnostic_index, title, edits }` + the enclosing
`outcome.type_id` + the linked `AnalyzerDiagnostic` (via `diagnostic_index`)
into the LSP `SyntaxFix` shape (defined in Layer 4). `RewriteEdit` →
`SyntaxTextEdit` is a direct enum mirror.

### 2.5 `context.rs` — intern source text

`ContextArena::compilation` (`context.rs:77-90`) adds
`entry_source_text: self.intern(input.source)` to the `ModCompilation`
literal at `:82-89`. The `empty()` path (`context.rs:47-59`) passes
`source: ""` which interns to an empty `BeskidStr`.

### 2.6 `analysis/diagnostics.rs` — add `origin` to `SemanticDiagnostic`

**Blast radius: 6 files / 15 struct-literal sites** (see §0). Run GitNexus
`impact({target: "SemanticDiagnostic", direction: "upstream"})` before editing.

Add to `SemanticDiagnostic` (`analysis/diagnostics.rs:22-33`):

```
pub struct SemanticDiagnostic {
    // ... existing fields ...
    pub origin: Option<String>,   // NEW — None = compiler, Some("beskid:mod:<type_id>") = mod
    pub severity: Severity,
}
```

`origin` is `Option<String>` so existing compiler-diagnostic constructors
default to `None` (compiler origin). All 15 literal sites must add
`origin: None` (compiler path) or `origin: Some(...)` (mod path). The
`analyzer_diagnostic_to_semantic` site (`mod_host/diagnostics.rs:166-174`) sets
`origin: Some(format!("beskid:mod:{type_id}"))`.

This is the single invasive change in the host. It is required because the
prepare spine flattens mod + compiler diagnostics into one
`Vec<SemanticDiagnostic>` (`prepare.rs:155,261,318`), and LSP needs to
distinguish them to route code actions. Encoding the origin in `code` is
unsafe (codes can collide across mods); encoding it in `help` is not a
routing channel.

### 2.7 `services/prepare.rs` — lift mod diagnostics out of the `collect_diagnostics` guard (G7)

This is the core build-failure fix. Today
`run_prepare_spine` (`prepare.rs:168-374`):

- runs `run_analyze_rewrite_after_composition` at `:286-292` (always),
- consumes only `mod_rewrite.program` at `:293` (always),
- maps `analyzer_outcomes` → diagnostics **only inside `if collect_diagnostics`**
  at `:295-306`.

So the typed/codegen path (`prepare_compilation` at `:108-130`, which calls the
spine with `collect_diagnostics=false` at `:126`) computes mod diagnostics and
throws them away. A mod `Error` does not fail the build.

**Redesign.** Extract a helper:

```
fn collect_analyzer_diagnostics(
    mod_rewrite: &ModHostAnalyzeResult,
    entry_unit: &ProgramUnit,
    entry_source: &str,
) -> Vec<SemanticDiagnostic> {
    // For each outcome, for each diagnostic, call analyzer_diagnostic_to_semantic
    // (which now sets origin = Some("beskid:mod:<type_id>")).
}
```

Then in `run_prepare_spine`, after `program = mod_rewrite.program;` (`:293`),
replace the guarded block `:295-306` with:

```
let analyzer_diagnostics = collect_analyzer_diagnostics(&mod_rewrite, entry_unit, entry_source);

if collect_diagnostics {
    collected_diagnostics.extend(analyzer_diagnostics);
} else {
    // Typed/codegen path: mod Errors fail the build; Warnings/Notes are dropped
    // (mirrors require_no_semantic_errors at :264 for compiler diagnostics).
    require_no_semantic_errors(&analyzer_diagnostics)?;
}
```

**Severity → build-failure mapping:**
- `Error` → build fails (via `require_no_semantic_errors`, which errors on
  `Severity::Error` — same gate used for compiler semantic errors at `:264`).
- `Warning` / `Note` → never fail the build. In the `collect_diagnostics` path
  they surface to LSP; in the typed/codegen path they are dropped (the build
  path does not collect warnings today — `:264` errors-out on Error and
  discards the rest of the semantic slice).

This matches the existing compiler-diagnostic contract: `require_no_semantic_errors`
(`semantic.rs`, used at `prepare.rs:264,316`) treats only `Severity::Error` as
fatal. Mod diagnostics get the identical contract.

### 2.8 `services/prepare.rs` — carry mod fixes out of the spine

`prepare_compilation_diagnostics` (`prepare.rs:134-157`) returns
`(PreparedCompilation, Vec<SemanticDiagnostic>)`. To carry mod fixes to LSP,
widen its return to `(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<SyntaxFix>)`
and have `run_prepare_spine` collect fixes from `mod_rewrite.analyzer_outcomes`
via `analyzer_fix_to_syntax_fix` (2.4). `PrepareSpineOutput` (`:159-162`) gains a
`collected_fixes: Vec<SyntaxFix>` field.

The typed/codegen path (`prepare_compilation` at `:108-130`) does **not** need
fixes — fixes are an IDE concern, not a build concern. It continues to return
`PreparedCompilation` only.

### 2.9 Touch summary for Layer 2

| File:line | Change |
|-----------|--------|
| `beskid_analysis/src/mod_host/invoker.rs:49-54` | `AnalyzerOutcome.fix_targets` → `fixes: Vec<AnalyzerFix>`; add `AnalyzerFix` |
| `beskid_analysis/src/mod_host/invoker.rs:246-260,402-416` | stub/scripted invokers: `fix_targets` → `fixes`; add `with_analyzer_fix` |
| `beskid_analysis/src/mod_host/invoker.rs:479-497,517-523` | tests update |
| `beskid_analysis/src/mod_host/native.rs:151` | forward `semantic: ModSemanticHandle { null, 0 }` |
| `beskid_analysis/src/mod_host/native.rs:164-166` | unmarshal `result.fixes` → `AnalyzerFix` |
| `beskid_analysis/src/mod_host/native.rs` (new fn) | `unmarshal_fixes` (reuses `unmarshal_edits` at `:294-312`) |
| `beskid_analysis/src/mod_host/native.rs:360-380` | test helper adds `entry_source_text` |
| `beskid_analysis/src/mod_host/context.rs:82-89` | intern `entry_source_text: self.intern(input.source)` |
| `beskid_analysis/src/mod_host/diagnostics.rs:151-175` | set `origin: Some("beskid:mod:<type_id>")`; add `analyzer_fix_to_syntax_fix` |
| `beskid_analysis/src/analysis/diagnostics.rs:22-33` | add `origin: Option<String>` to `SemanticDiagnostic` (+ 15 literal sites) |
| `beskid_analysis/src/services/prepare.rs:159-162` | `PrepareSpineOutput` gains `collected_fixes` |
| `beskid_analysis/src/services/prepare.rs:286-306` | lift analyzer diagnostics out of `if collect_diagnostics`; add `require_no_semantic_errors` for mod Errors on the typed path |
| `beskid_analysis/src/services/prepare.rs:134-157` | `prepare_compilation_diagnostics` returns `(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<SyntaxFix>)` |

**Impact (must run before editing):**
- `impact({target: "AnalyzerOutcome", direction: "upstream"})` — touches
  `native.rs`, `stub/scripted invokers`, `analyze.rs`, `api.rs`, `prepare.rs`,
  `beskid_engine`/`beskid_tests` consumers.
- `impact({target: "SemanticDiagnostic", direction: "upstream"})` — 15 literal
  sites across 6 files; the miette `#[derive(Error, Diagnostic)]` at
  `analysis/diagnostics.rs:20-21` must keep compiling (the new field is not a
  miette attribute, so the derive is unaffected).
- `impact({target: "prepare_compilation_diagnostics", direction: "upstream"})`
  — called by `beskid_queries` (`diagnostics.rs:55`) and LSP
  (`beskid_lsp/src/diagnostics.rs:55`); the return-type widening ripples to both.

---

## 3. SDK mirror — `compiler/corelib/packages/compiler-sdk/`

### 3.1 `Collect.bd` — make `AnalysisResult` non-empty

`Collect.bd:28` (`pub type AnalysisResult {}`) becomes:

```
pub type Diagnostic {
    string code,
    string message,
    Beskid.Compiler.Diagnostics.Severity severity,
    u64 spanStart,
    u64 spanEnd,
}

pub type Edit {
    // mirrors ModEdit (mod_contract.rs:213-224): 0=Insert, 1=Replace, 2=Delete
    u32 kind,
    u64 start,
    u64 end,
    string text,
}

pub type QuickFix {
    u32 diagnosticIndex,   // indexes into the diagnostics array returned alongside
    string title,
    Edit[] edits,
}

pub type AnalysisResult {
    Diagnostic[] diagnostics,
    QuickFix[] fixes,
}
```

`AnalysisRequest` (`Collect.bd:24-26`) is **unchanged** in Phase 1. The
`semantic` handle is a host-side ABI concern (`ModSemanticHandle`); it is not
exposed in the Beskid-side `AnalysisRequest` until Option A lands (Phase 2),
at which point `AnalysisRequest` gains a `Beskid.Compiler.Semantic.SemanticHandle`
field and a `Beskid.Compiler.Semantic` module is added (3.4).

`FixError` (`Collect.bd:33`) stays empty for now — it is the `Rewriter` error
channel, unrelated to analyzer quick-fixes.

`CollectFacadeVersion` (`Collect.bd:91-93`) bumps `0.4.0` → `0.5.0` to signal
the `AnalysisResult` break.

### 3.2 `Diagnostics.bd` — add `Diagnostic`/`QuickFix`/`Edit`?

**Decision: keep `Diagnostic`/`QuickFix`/`Edit` in `Collect.bd`, not `Diagnostics.bd`.**

`Diagnostics.bd` (`compiler-sdk/.../Diagnostics.bd`) is **reflect-generated**
from Rust enums (`Severity`, `SemanticIssueKind`) by `regen_mod_sdk_surfaces.sh:48-66`
via `beskid_ast_reflect_gen`. The reflect pipeline emits enums annotated with
`#[beskid_ast_derive::beskid_reflect]` (`compiler_sdk_reflect.rs:101-128`).
`Diagnostic`/`QuickFix`/`Edit` are **structs with primitive fields**, not
reflect-tagged enums — they mirror ABI structs (`ModDiagnostic`,
`ModQuickFix`, `ModEdit`), not compiler-internal enums. Putting them in the
reflect-generated `Diagnostics.bd` would require either (a) adding
`#[beskid_reflect]` struct support to `beskid_ast_reflect_gen` (it currently
emits enums — see `compiler_sdk_reflect.rs:101-128`, all four annotated types
are enums), or (b) hand-appending them after the reflect block, which fights
the "Rust sources are authoritative" regeneration contract
(`Diagnostics.bd:1-2`).

Instead, `Collect.bd` is a **hand-maintained facade** (regenerated verbatim
from a heredoc — `regen_mod_sdk_surfaces.sh:203-285`), so adding structs there
is the natural home and does not touch the reflect pipeline. The
`Diagnostic`/`QuickFix`/`Edit` types are co-located with the `Analyzer`
contract that produces them.

`Diagnostics.bd` keeps its current role: the `Severity` enum (used by
`Diagnostic.severity`) and the `SemanticIssueKind` catalog. No change to
`Diagnostics.bd` in this phase. (An emit helper is deferred — see 3.5.)

### 3.3 `Query.bd` — no change in Phase 1 (Option C)

`Query.bd` (`Query.bd:1-146`) is syntax-only (G6). Under Option C, mods
re-derive diagnostics from the syntax surface already exposed here:
`At`/`AtProgram` (`:28-29`), `Descendants`/`Children`/`Parent`/`Ancestors`
(`:31-34`), `Span`/`TrySpan` (`:35-36`), `OfKind`/`FindFirst`/`Select`/`WhereKind`
(`:38-41`), and the `As*` node casts (`:49-135`). No `Query.bd` change in
Phase 1.

### 3.4 `Query.bd` / new `Semantic.bd` — Phase 2 (Option A) sketch

When Option A lands, add a new `Beskid.Compiler.Semantic` module (hand-maintained
facade, mirroring `Collect.bd`'s heredoc style) exposing:

```
pub type SemanticHandle { /* opaque, mirrors ModSemanticHandle */ }
pub type TypeId { string path }
pub type Field { string name, TypeId type }
pub contract Semantic {
    Option<TypeId> ResolveType(string path);
    Field[] TypeFields(TypeId type);
    Option<TypeId> ContextualExpectedType(Beskid.Syntax.Nodes.NodeRef node);
}
```

and widen `AnalysisRequest` (`Collect.bd:24-26`) with a `SemanticHandle`
field. The host populates the `ModSemanticHandle` vtable to route these to
`beskid_queries`. This is **out of scope for this design phase** but the ABI
shape in §1.2 is forward-compatible with it.

### 3.5 Emit helper — deferred

An ergonomic `emit` helper (e.g. `Diagnostics.Emit(diagnostic, fixes)`) is
deferred. The `Analyzer.Analyze` contract (`Collect.bd:77-79`) returns an
`AnalysisResult` directly; mods construct `Diagnostic`/`QuickFix` literals.
A helper can be added later in `Diagnostics.bd`'s hand-maintained facade
(`Diagnostics.bd:151-157`) without an ABI change.

### 3.6 Regeneration

`Collect.bd` is emitted by `write_collect` (`regen_mod_sdk_surfaces.sh:203-285`).
The heredoc at `:236` (`pub type AnalysisResult {}`) and `:241`
(`pub type FixError {}`) are updated to the new shapes, and `:281`
(`CollectFacadeVersion` `0.3.0` — note the script heredoc says `0.3.0` while
the checked-in `Collect.bd:92` says `0.4.0`; reconcile to `0.5.0`). Re-run
`regen_mod_sdk_surfaces.sh` to regenerate `Collect.bd`. `Diagnostics.bd` and
`Query.bd` are unchanged, so their regen steps (`:48-66`, `:34-46`) need no
edits.

### 3.7 Touch summary for Layer 3

| File:line | Change |
|-----------|--------|
| `compiler-sdk/src/Beskid/Compiler/Collect.bd:24-26` | (Phase 2 only) add `SemanticHandle` to `AnalysisRequest` |
| `compiler-sdk/src/Beskid/Compiler/Collect.bd:28` | replace `AnalysisResult {}` with `Diagnostic`/`Edit`/`QuickFix`/`AnalysisResult` structs |
| `compiler-sdk/src/Beskid/Compiler/Collect.bd:91-93` | bump `CollectFacadeVersion` → `0.5.0` |
| `compiler-sdk/regen_mod_sdk_surfaces.sh:236,241,281` | mirror the `Collect.bd` changes in the `write_collect` heredoc; bump version |
| `compiler-sdk/src/Beskid/Compiler/Diagnostics.bd` | no change (Phase 1) |
| `compiler-sdk/src/Beskid/Compiler/Query.bd` | no change (Phase 1) |

---

## 4. LSP changes — `compiler/crates/beskid_lsp/`

### 4.1 `session/store.rs` — add `SyntaxFix` + `syntax_fixes` on `Document`

New generation-bound fact type, mirroring `SyntaxDiagnostic`
(`store.rs:54-60`):

```
pub struct SyntaxTextEdit {
    pub kind: SyntaxTextEditKind,   // Insert/Replace/Delete
    pub start: usize,
    pub end: usize,
    pub text: String,
}

pub enum SyntaxTextEditKind { Insert, Replace, Delete }

pub struct SyntaxFix {
    pub source: String,            // "beskid:mod:<type_id>" for mod fixes; "beskid" for compiler fixes (future)
    pub diagnostic_code: String,   // links to the SyntaxDiagnostic.code this fix addresses
    pub title: String,
    pub edits: Vec<SyntaxTextEdit>,
}
```

`Document` gains `pub syntax_fixes: Vec<SyntaxFix>` (alongside
`syntax_diagnostics`). `clear_syntax_facts` (`store.rs:41-49`) also clears
`syntax_fixes` (fail-closed on hard invalidation, matching the existing
contract at `:40-49`).

### 4.2 `diagnostics.rs` — propagate origin + collect fixes

`SyntaxDiagnostic` (`store.rs:54-60`) gains `pub source: String` (default
`"beskid"`). `syntax_diagnostic_from_semantic` (`diagnostics.rs:121-136`) maps
`SemanticDiagnostic.origin` → `SyntaxDiagnostic.source` (`None` → `"beskid"`,
`Some(s)` → `s`). `syntax_to_lsp_diagnostic` (`diagnostics.rs:138-151`) uses
`fact.source` instead of the hardcoded `"beskid"` at `:147`.

`collect_syntax_diagnostics` (`diagnostics.rs:26-69`) currently returns
`Vec<SyntaxDiagnostic>`. It must also produce `Vec<SyntaxFix>`. Two options:

- **(a)** Widen the return to `(Vec<SyntaxDiagnostic>, Vec<SyntaxFix>)` and have
  callers store both on the `Document`.
- **(b)** Add a sibling `collect_syntax_fixes` that re-runs the prepare spine.

Option (a) is correct — re-running the prepare spine (b) doubles the work and
risks generation mismatch. The prepare-spine call at `diagnostics.rs:55-63`
already returns `(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<SyntaxFix>)`
after the Layer 2.8 widening; `collect_syntax_diagnostics` returns both and the
publish path stores them on the `Document` together (same generation).

### 4.3 `features/code_actions/handler.rs` — replace closed match with a provider registry

Today `handle_code_actions` (`handler.rs:47-113`) hardcodes a `match` on
compiler codes (`:64-97`). Replace with a registry of `CodeActionProvider`s:

```
trait CodeActionProvider: Send + Sync {
    /// Does this provider own fixes for (source, code)?
    fn handles(&self, source: &str, code: &str) -> bool;
    /// Build the LSP CodeAction for the given diagnostic, reading fix data
    /// from the Document's generation-bound facts.
    fn build(&self, uri: &Uri, doc: &Document, diag: &Diagnostic) -> Option<CodeAction>;
}
```

Registry (constructed once, e.g. in `server/init.rs` or a `code_actions/registry.rs`):

```
fn code_action_providers() -> Vec<Box<dyn CodeActionProvider>> {
    vec![
        Box::new(CompilerRemoveLinesProvider),      // W1503 (handler.rs:115-129)
        Box::new(CompilerRemoveRangeProvider),      // W1639 (handler.rs:131-145)
        Box::new(CompilerDocCommentProvider),       // W1610-W1625 (handler.rs:15-44,88-96)
        Box::new(ModQuickFixProvider),               // NEW — mod-supplied fixes
    ]
}
```

The existing compiler handlers (`remove_lines_action` `:115-129`,
`remove_range_action` `:131-145`, `doc_comment_code_action` `:15-44`) become
thin providers keyed by `source == "beskid"` and their specific codes. Their
behavior is unchanged — this is a structural refactor, not a behavioral one.

`handle_code_actions` (`:47-113`) becomes:

```
pub fn handle_code_actions(uri, doc, params) -> CodeActionResponse {
    let providers = code_action_providers();
    let mut actions = Vec::new();
    // format-document source action (unchanged, :50-60)
    // ...
    for diag in &params.context.diagnostics {
        let source = diag.source.as_deref().unwrap_or("beskid");
        let code = match &diag.code { Some(NumberOrString::String(c)) => c, _ => continue };
        for provider in &providers {
            if provider.handles(source, code) {
                if let Some(action) = provider.build(uri, doc, diag) {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }
        }
    }
    // doc-comment source assist (unchanged, :100-109)
    CodeActionResponse::from(actions)
}
```

### 4.4 `ModQuickFixProvider` — route mod fixes from `Document.syntax_fixes`

```
struct ModQuickFixProvider;

impl CodeActionProvider for ModQuickFixProvider {
    fn handles(&self, source: &str, _code: &str) -> bool {
        source.starts_with("beskid:mod:")   // any mod-origin diagnostic
    }
    fn build(&self, uri, doc, diag) -> Option<CodeAction> {
        let source = diag.source.as_ref()?;
        let code = match &diag.code { Some(NumberOrString::String(c)) => c, _ => return None };
        // Find the generation-bound fix matching (source, code).
        let fix = doc.syntax_fixes.iter().find(|f| f.source == *source && f.diagnostic_code == *code)?;
        // Convert SyntaxTextEdit[] -> LSP TextEdit[] (byte offsets -> positions).
        let lsp_edits: Vec<TextEdit> = fix.edits.iter().map(|e| text_edit_to_lsp(&doc.text, e)).collect();
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), lsp_edits);
        Some(CodeAction {
            title: fix.title.clone(),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diag.clone()]),
            edit: Some(WorkspaceEdit { changes: Some(changes), ..Default::default() }),
            ..CodeAction::default()
        })
    }
}
```

This satisfies the LSP contract: a `QUICKFIX` action keyed by
`(source = "beskid:mod:<type_id>", code)`, carrying the diagnostic it fixes
and the workspace edit. The `source` tag is the routing key that distinguishes
mod fixes from compiler fixes (G8).

### 4.5 `server/init.rs` — no capability change

`code_action_provider: Some(CodeActionProviderCapability::Simple(true))`
(`init.rs:29`) stays `Simple(true)` — the registry is an internal
implementation detail; the LSP capability shape is unchanged. The provider
list is constructed inside `handle_code_actions`, not advertised to the client.

### 4.6 Touch summary for Layer 4

| File:line | Change |
|-----------|--------|
| `beskid_lsp/src/session/store.rs:54-60` | add `source: String` to `SyntaxDiagnostic`; add `SyntaxFix`/`SyntaxTextEdit`/`SyntaxTextEditKind` |
| `beskid_lsp/src/session/store.rs` (Document) | add `syntax_fixes: Vec<SyntaxFix>` |
| `beskid_lsp/src/session/store.rs:41-49` | `clear_syntax_facts` clears `syntax_fixes` |
| `beskid_lsp/src/diagnostics.rs:26-69` | `collect_syntax_diagnostics` returns `(Vec<SyntaxDiagnostic>, Vec<SyntaxFix>)`; stores fixes from prepare spine |
| `beskid_lsp/src/diagnostics.rs:121-136` | `syntax_diagnostic_from_semantic` maps `origin` → `source` |
| `beskid_lsp/src/diagnostics.rs:138-151` | `syntax_to_lsp_diagnostic` uses `fact.source` (not hardcoded `"beskid"`) |
| `beskid_lsp/src/features/code_actions/handler.rs:47-113` | replace closed `match` with `CodeActionProvider` registry loop |
| `beskid_lsp/src/features/code_actions/handler.rs:15-44,115-145` | existing handlers become `Compiler*Provider` impls |
| `beskid_lsp/src/features/code_actions/` (new) | `ModQuickFixProvider` + `registry.rs` |
| `beskid_lsp/src/server/init.rs:29` | no change (registry is internal) |

**Impact (must run before editing):**
- `impact({target: "handle_code_actions", direction: "upstream"})` — sole caller
  is `server/backend.rs:267`.
- `impact({target: "collect_syntax_diagnostics", direction: "upstream"})` —
  called by the publish/refresh path and tests (`diagnostics.rs:55,197,266`).
- `impact({target: "SyntaxDiagnostic", direction: "upstream"})` — constructed
  in `diagnostics.rs` and compared in tests (`diagnostics.rs:282-293`).

---

## 5. Sequencing — dependency order and independent shippables

The layers have a strict bottom-up dependency for the *full* boundary, but
two slices can ship independently:

```
Layer 1 (ABI)  ──┬──► Layer 2 (host) ──┬──► Layer 4 (LSP)
                 │                      │
                 └──► Layer 3 (SDK) ───┘
```

**Slice A — "Mod Errors fail builds" (G7 fix, no quick-fixes).**
Ships with Layer 1.1 (fixes ABI) deferred to Slice B. Touches:
- `prepare.rs:286-306` lift + `require_no_semantic_errors` (2.7),
- `analysis/diagnostics.rs` `origin` field (2.6) — needed so LSP can later route,
- `mod_host/diagnostics.rs:151-175` set origin (2.4).
This slice makes mod `Error`-severity diagnostics fail the typed/codegen build
and surface in LSP diagnostics. No code actions yet. Independently shippable
and independently testable (`cargo test -p beskid_analysis`).

**Slice B — "Quick-fixes end-to-end".**
Builds on Slice A. Touches:
- Layer 1.1 (`ModQuickFix`/`ModQuickFixSlice`/`ModAnalysisResult.fixes`),
- Layer 2.1–2.4 (`AnalyzerFix`, `unmarshal_fixes`, `analyzer_fix_to_syntax_fix`),
- Layer 2.8 (`prepare_compilation_diagnostics` returns fixes),
- Layer 3.1 (`Collect.bd` `AnalysisResult` non-empty),
- Layer 4.1–4.4 (`SyntaxFix`, `ModQuickFixProvider`, registry).
Ships after Slice A is green. Testable via `beskid_tests` scripted invoker
(`ScriptedContractInvoker::with_analyzer_fix`, mirroring `with_analyzer_diagnostic`
at `invoker.rs:345-348`) and an LSP code-action test mirroring
`handler.rs:208-246`.

**Slice C — "Source text across the ABI" (1.3).**
`entry_source_text` on `ModCompilation` is ABI-additive and independent of
Slices A/B. Can ship in the same PR as Slice A (it touches `context.rs:82-89`
and `native.rs:360-380` test helper) or separately. Low risk.

**Slice D — "Semantic query surface" (Option A, Phase 2).**
Out of scope for this design. The `ModSemanticHandle` ABI slot (1.2) is
reserved and null in Phase 1 so Slice D can land later without a layout break.

Recommended PR order: **Slice A + C** together (host-only, no SDK break),
then **Slice B** (ABI + SDK + LSP, the breaking change), then Slice D later.

---

## 6. Open blockers / human decisions

1. **`SemanticDiagnostic.origin` field addition (2.6).** This is the one
   invasive host change (15 literal sites / 6 files). It is required for LSP
   routing. **Decision needed:** confirm adding `origin: Option<String>` to
   `SemanticDiagnostic` is acceptable, or whether the team prefers encoding
   origin out-of-band (e.g. a parallel `Vec<(usize, String)>` of
   `(diagnostic_index, source_tag)` returned from the prepare spine). The
   parallel-channel approach avoids touching `SemanticDiagnostic` but
   complicates every caller. **Recommendation:** add the field; run
   `impact({target: "SemanticDiagnostic"})` first.

2. **`CollectFacadeVersion` / ABI break signaling.** Adding fields to
   `ModCompilation` and `ModAnalysisRequest` (both `#[repr(C)]`) breaks
   existing native mod artifacts. The ABI is pre-1.0 (`Collect.bd:92` is
   `0.4.0`). **Decision needed:** confirm a minor version bump (`0.5.0`) is
   the break signal and that no shipped mod artifacts exist that must keep
   working. (The native invoker today falls back to stub when the object is
   missing — `native.rs:81-84` — so the break is graceful for absent mods,
   but present mods must be recompiled.)

3. **Snapshot exposure scope (Option A vs C).** This design defers semantic
   exposure (Option C). **Decision needed:** confirm that "missing fields"
   diagnostics (which need resolved struct types) are acceptable to defer to
   Phase 2, or whether Phase 1 must include at least a minimal
   `ResolveType`/`TypeFields` callback. If Phase 1 must include semantic
   queries, the `ModSemanticHandle` vtable (1.2) and `Beskid.Compiler.Semantic`
   SDK module (3.4) become in-scope and the effort roughly doubles.

4. **Mod fix deduplication / ordering.** When multiple mods register
   `Analyzer` contracts for overlapping spans, their fixes may conflict. The
   LSP `ModQuickFixProvider` (4.4) returns the first matching fix per
   `(source, code)`. **Decision needed:** is first-match acceptable, or does
   the host need to dedupe/order fixes (e.g. by `type_id` then
   `diagnostic_index`)? The current `AnalyzerOutcome` ordering follows
   `session.registrations()` iteration order (`analyze.rs:25-34`), which
   follows `mod.load` discovery order — not deterministic across workspace
   reshuffles. **Recommendation:** first-match for Phase 1; document the
   non-determinism and revisit if mods collide in practice.

5. **`Diagnostics.bd` reflect pipeline for structs.** This design keeps
   `Diagnostic`/`QuickFix`/`Edit` in the hand-maintained `Collect.bd` (3.2)
   to avoid extending `beskid_ast_reflect_gen` to emit structs. **Decision
   needed:** confirm that hand-maintained struct facades in `Collect.bd` are
   acceptable, or whether the reflect generator should learn to emit
   `#[beskid_reflect]` structs (currently it emits only enums —
   `compiler_sdk_reflect.rs:101-128`). Extending the generator is a larger
   effort and is not required for this boundary.

6. **`regen_mod_sdk_surfaces.sh` version drift.** The script heredoc emits
   `CollectFacadeVersion` `0.3.0` (`regen_mod_sdk_surfaces.sh:281`) but the
   checked-in `Collect.bd:92` says `0.4.0` — the script is stale relative to
   the hand-maintained file. **Decision needed:** reconcile (the script
   should be the source of truth per `Collect.bd:1-2`), then bump to `0.5.0`
   as part of Slice B.

---

## 7. End-to-end trace (Phase 1, Slice A+B)

1. Mod `Analyzer.Analyze(AnalysisRequest)` runs in a dlopen'd native artifact
   (`native.rs:143-170`). It reads `context.compilation.entry_source_text`
   (1.3, no disk I/O) and queries syntax via `Query.bd` (syntax-only, G6). It
   returns a `ModAnalysisResult { diagnostics, fixes }` (1.1).
2. `NativeContractInvoker::invoke_analyzer` (`native.rs:143-170`) unmarshals
   diagnostics (`unmarshal_diagnostics` `:261-291`) and fixes (new
   `unmarshal_fixes`, 2.2) into `AnalyzerOutcome { type_id, diagnostics,
   fixes }` (2.1).
3. `run_analyzers` (`analyze.rs:12-38`) collects outcomes;
   `run_analyze_rewrite_after_composition` (`api.rs:238-253`) returns them in
   `ModHostAnalyzeResult.analyzer_outcomes` (`types.rs:138-149`).
4. `run_prepare_spine` (`prepare.rs:168-374`) calls
   `collect_analyzer_diagnostics` (2.7) after `mod_rewrite` (`:293`).
   - Typed/codegen path (`collect_diagnostics=false`):
     `require_no_semantic_errors(&analyzer_diagnostics)?` — mod `Error` fails
     the build (2.7).
   - Diagnostics path (`collect_diagnostics=true`): diagnostics extend
     `collected_diagnostics`; fixes extend `collected_fixes` (2.8).
   `analyzer_diagnostic_to_semantic` (`mod_host/diagnostics.rs:151-175`) sets
   `origin: Some("beskid:mod:<type_id>")` (2.4, 2.6).
5. `prepare_compilation_diagnostics` (`prepare.rs:134-157`) returns
   `(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<SyntaxFix>)` (2.8).
6. LSP `collect_syntax_diagnostics` (`diagnostics.rs:26-69`) calls the prepare
   spine (`:55-63`), maps `SemanticDiagnostic.origin` → `SyntaxDiagnostic.source`
   (`:121-136`) and stores `SyntaxFix[]` on `Document.syntax_fixes` (4.1, 4.2).
7. LSP publishes diagnostics via `lsp_diagnostics_from_syntax`
   (`diagnostics.rs:72-74`); `syntax_to_lsp_diagnostic` (`:138-151`) emits
   `source: "beskid:mod:<type_id>"` (4.2).
8. Client requests code actions for a diagnostic. `handle_code_actions`
   (`handler.rs:47-113`) iterates the `CodeActionProvider` registry (4.3).
   `ModQuickFixProvider.handles("beskid:mod:...", code)` returns true (4.4);
   `build` reads `doc.syntax_fixes`, finds the matching `(source, code)`,
   converts `SyntaxTextEdit[]` → LSP `TextEdit[]`, and returns a
   `CodeAction { kind: QUICKFIX, diagnostics: [diag], edit }` (4.4).
9. Client applies the edit. The mod's fix is applied to the source.

This closes G1–G9: mods emit structured diagnostics with spans (G1, G2, G9),
return quick-fixes (G2, G4), diagnostics always surface and mod Errors fail
builds (G7), and quick-fixes surface in LSP as `QUICKFIX` actions keyed by
`(source, code)` (G8). Source text crosses the ABI without disk I/O (1.3).
Semantic exposure is deferred to Phase 2 with a forward-compatible ABI slot
(1.2, G5, G6).
