# The query layer is the only front end for executable builds (0.6.0)

Status: design and implementation on `v06/front-end-authority`. Informative; the normative text is
the OpenSpec change `v06-query-front-end` (staged under `docs/openspec-staging/` in this
repository because OpenSpec lives in the root repository).

Decision closed here: the legacy `beskid_analysis::types::TypeChecker` is retired from every path.
The generation-bound `beskid_queries` facts are the one typing authority for `beskid check`,
`analyze`, the language server, and lowering, as `rustc` has one `typeck` whose results both
diagnostics and MIR building read, and as `rust-analyzer` has one `hir_ty::InferenceResult` that
hover, diagnostics and assists share.

## 1. What lowering still consumed from the legacy typed result

Every claim below was verified against the tree at `ad260f6e`.

### 1.1 The product

`crates/beskid_analysis/src/services/front_end.rs` defined

```
pub struct FrontEndTypedResult {
    pub assembly: ProgramAssembly,
    pub program: Spanned<Program>,            // post-Mod-rewrite entry syntax
    pub resolution: crate::resolve::Resolution,
    pub typed: crate::types::TypeResult,
    pub binding_plan: BindingPlan,
    pub composition_snapshot: CompositionSnapshot,
}
```

`resolution` and `typed` were produced by `resolve_and_type_program_with_assembly`
(`services/semantic_facts.rs`), which ran `ModuleIndex::resolve_entry_program` and then
`TypeChecker::check_entry` under the pipeline phases `lower.resolve` and `lower.type_check`
(`services/prepare/spine.rs`, `LOWER` block).

### 1.2 Readers of `FrontEndTypedResult.typed` and `.resolution`

| Reader | What it read | Verdict |
| --- | --- | --- |
| `beskid_codegen::prepared_syntax` (every `lower_prepared_*`) | `front.syntax_assembly()`, `front.binding_plan`, `front.composition_snapshot` only | Never read `typed` or `resolution`. `TypedProgram` is built by `build_typed_program_with_corelib_syscall_services` (or the runtime-fixture constructor) from the syntax assembly; `CodegenInput` holds that `TypedProgram`, the roots, target, manifest, composition authority and glue bindings. |
| `beskid_aot::prepared_syntax`, `beskid_engine::services::{jit_preparation, entrypoint_execution, syntax_queries}`, `beskid_cli::commands::{build, run, clif, syntax_codegen, prepared_matrix, compiler_mod, build/glue_rust}`, `beskid_codegen::native_mod::producer` | pass `&FrontEndTypedResult` through to codegen; `syntax_queries` reads `front.syntax_assembly()` | Never read `typed` or `resolution`. |
| `services/prepare/spine.rs` | `typed_fingerprint(&resolution)` and `typed_fingerprint_types(&typed)` into `SemanticSnapshot::with_typed_resolution` | Cache bookkeeping only; nothing reads the two fingerprints back (`session.rs` stores them). |
| `services/front_end.rs` `Debug` impl | prints both | Cosmetic. |
| `beskid_queries::output::SharedTypeResult` | wraps `Arc<TypeResult>` | No construction site anywhere in the workspace. |
| `beskid_codegen::artifact::CodegenArtifact::type_descriptors: HashMap<beskid_analysis::types::TypeId, TypeDescriptorData>` and `emit_type_descriptors` | HIR-era GC descriptors keyed by legacy `TypeId` | No writer anywhere (`Default` only); the emitters iterate an always-empty map. Dead. |
| `services/unit_ops.rs` (`type_entry`, `type_entry_gate`, `type_dep_signatures`, `type_unit_body`, `type_unit_signatures`) | legacy `TypeResult` and `UnitTypeSurface` | Used only by tests (`beskid_tests_projects` fixture harness, `beskid_queries/tests/runtime_fixture.rs`) and the deprecated `services::queries` alias. |
| `analysis/rules/staged/error_handling.rs` (`stage7_error_handling`) | `TypeChecker::invalid_try_expression_spans` through `types/try_desugar.rs` | Skipped whenever `defer_try_diagnostics` is set, which every project-backed path sets because the spine requires the query authority (`run_prepare_spine` fails closed without it). Live only for the project-less `analyze_program` path, where the query gate already owns E1222 (`try_expression_judgement`). |

So the legacy checker's only remaining role in lowering was as a second gate: a `TypeError` made
`into_executable()` fail with "executable front end rejected a program the semantic gate
accepted", and every field it produced was discarded. The constructs that failed `beskid build`
(named functions as values, function-typed fields, calls on call results, `this.Method`, some
generic inference) fail only because that gate predates the facts the query layer and ISLE already
implement (`function_item_value`, `CallLowering::FunctionValue`, `expected_type`).

### 1.3 What each former `TypeResult` field is answered by today

| Legacy field | Query fact (crate `beskid_queries`) |
| --- | --- |
| `types: TypeTable`, `node_types[AstNodeId]` | `node_type`, `value_abi_type`, `abi_type`, `expected_type`, `generic_source_expression_identity` (source identity, ABI derived from it) |
| `local_types[LocalId]` | `resolved_local`, `local_slot`, `abi_local_declaration_type`, `generic_source_local_identity`, `lambda_source_identity` |
| `function_signatures`, `method_function_signatures` | `item_signature`, `item_abi_signature`, `callable_signature`, `call_abi_signature`, `generic_call_specialization` |
| `struct_fields_ordered`, `struct_event_fields` | `aggregate_layout`, `aggregate_field_access`, `aggregate_literal_declaration`, `event_field_layout` |
| `enum_variants_ordered` | `enum_layout`, `enum_constructor`, `enum_match`, `pattern_binding` |
| `generic_items`, `unit_surfaces` | `generic_specialization_instance`, specialization environments in `abi/specialization`, `contract_witnesses_for_call` |
| `lowering.call_kinds[AstNodeId]` (`CallLoweringKind`) | `call_lowering` (`CallLowering::{Direct, FunctionValue, Runtime, ManifestBuiltin, NativeModCallback, CorelibService, ...}`), `bulk_parameter`, `primitive_numeric_conversion` |
| `lowering.cast_intents` | `cast_intents`, `contextual_integer_literal_abi_type` through `expected_type` |
| `Resolution` (resolver tables) | `resolved_item`, `resolved_local`, `unresolved_call_target`, `unresolved_type_reference`, `unresolved_imports`, `qualified_module_resolution`; the resolver itself (`beskid_analysis::resolve`) stays for the module index, the IDE document snapshot and the LSP navigation, and is not a typing authority |

### 1.4 The gates that remain (fail closed)

1. The query diagnostics gate on the prepare spine: `QueryModSemanticScope::fact_findings` runs
   `semantic_diagnostics_for_roots` (every own unit; `check_items` plus the direct-call closure,
   `check_typing_obligations`, `check_gate_obligations`, `check_unit_obligations`,
   `extern_profile_findings`, E1222). On the executable path an error finding fails preparation;
   on the diagnostics path `beskid_tools::session::executable_gate_prepared` fails on any error.
2. The codegen legality gate: `module_emission::orchestration` runs `check_items` over the
   requested and discovered keys before any item is emitted.
3. Emission: a fact that is unavailable is an internal error (E2101, E2102), inventoried by
   `beskid_queries/tests/unavailable_inventory.rs`; nothing falls back.

Removing the legacy gate does not open a fail-open path: every legacy `TypeError` class has a
query obligation and a parity fixture (`docs/reports/2026-10-05-v06-query-check-authority.md`,
phase 3), and the executable path runs the same authority as `check`.

## 2. Target architecture

```
assemble -> parse -> Mod generate -> structural rules (SemanticPipelineRule, every own unit)
        -> composition -> Mod analyze/rewrite -> query gate (semantic_diagnostics_for_roots)
        -> ExecutableFrontEnd { assembly, program, binding_plan, composition_snapshot }
        -> beskid_codegen::prepared_syntax: TypedProgram (build_typed_program_*) -> CodegenInput
        -> check_items -> ISLE (NodeFacts over beskid_queries facts) -> CLIF
```

- `FrontEndTypedResult` becomes `ExecutableFrontEnd` with the four fields codegen reads. It is a
  syntax and composition product; it carries no types. `PreparedCompilation.executable` is always
  present once the spine completes (it was `typed: Option<_>` only because the legacy gate could
  fail after the query gate accepted the program).
- The spine requires a `SemanticFactAuthority` on both paths. The authority-less
  `beskid_analysis::services::prepare_compilation` and `compile_front_end_with_pipeline` are
  removed; `beskid_queries::{prepare_compilation, prepare_compilation_diagnostics,
  compile_mod_free_front_end, compile_front_end_with_mod_host}` are the entry points.
- The project-less `beskid check` and `beskid analyze` path (`beskid_tools::pipeline::frontend`,
  `beskid_cli::commands::analyze`) also goes through the query gate, with
  `synthetic_compile_plan_for_source` and no Mods, instead of structural rules only.
- `DependencyTypingPolicy` (`EntryOnly` / `FullClosure`) controlled how many dependency bodies the
  legacy checker walked. The query gate judges own units and the direct-call closure of own items
  regardless, so the policy is removed from `PrepareOptions`. The language server keeps its two
  scheduling passes under its own name (`EditorPreparePass::{Immediate, Complete}`), which only
  decide whether structural diagnostics are collected on the immediate pass.
- Deleted from `beskid_analysis`: `types/` (checker, inference, lowering_prep, surface, table,
  display, path_value, result, try_desugar), `services/semantic_facts.rs`, `services/unit_ops.rs`
  and the deprecated `services::queries` alias, `rules/staged/error_handling.rs` (stage 7, the
  only structural rule that read the checker) with `AnalysisOptions::defer_try_diagnostics`, the
  `SemanticSnapshot` resolution and typed fingerprints. Deleted from `beskid_codegen`: the dead
  `type_descriptors` path (`TypeDescriptorData`, `emit_type_descriptors`, `DescriptorHandles`).
  Deleted from `beskid_pipeline`: the HIR-era phase ids `lower.ast`, `lower.resolve_pass1`,
  `lower.normalize`, `lower.resolve`, `lower.type_check`, and `semantic.error_handling`. `lower`
  stays as the instant boundary at which the executable front end exists.

### 2.1 Interaction with content-based persisted query keys

Another agent is making persisted Salsa keys content-based (`beskid_queries/src/persistence.rs`,
untouched here). This change adds no new persisted ingredient family: `for_iterator_fact_tracked`
already carries `#[salsa::tracked(persist)]`; its value type `ForIteratorFact` gains a `source`
field and the `local_declaration_type` arm for `for` variables changes the value of `node_type`
for array iterations from `Err(unavailable)` to `Ok`. Persisted values are gated by
`snapshot_format_version()` (crate version, `GRAMMAR_REVISION`, snapshot number), so a snapshot
written by an older compiler is rejected closed, never reinterpreted. No generation-dependent
identity is introduced: the new facts are keyed by `AstNodeKey` like every other fact and derive
everything from registered syntax.

## 3. Language features that the change makes buildable

### 3.1 Named functions as values, function-typed fields, calls on call results

Already lowered (`function_item_value`, `CallLowering::FunctionValue`,
`beskid_codegen/tests/isle_adapter/function_values.rs`). They now build end to end because the
spine no longer rejects them. Coverage: `beskid_codegen/tests/prepared_syntax.rs` lowers a program
through `compile_mod_free_front_end` for each construct.

### 3.2 `for x in T[]`

Decision: supported. Precedent: Rust `for x in &array` and Swift `for x in array` iterate in index
order; C# `foreach` over an array reads each element when its iteration begins and observes stores
to not-yet-visited elements because arrays are reference values. Beskid arrays are managed
reference values of fixed length, so the C# reading applies to stores and the Rust reading applies
to the loop variable (immutable per iteration; assigning it is E1214, as for `range`).

Semantics (normative text in the OpenSpec change):

- `for id in expr block` where `expr` has type `T[]` evaluates `expr` once, before the first
  iteration, to an array value; iterating a null array traps like an index read.
- Iterations run for indices `0 .. length` in increasing order. At the start of each iteration
  `id` is bound to the element at the current index, read from the array value captured at entry.
- `id` has type `T` and is immutable. `break` and `continue` target the loop. Reassigning the
  local the iterable named does not change the iteration.
- A `for` iterable whose type is neither `range(...)`, an array, nor a nominal type with the
  iterator protocol stays E1215.

Implementation:

- `beskid_queries`: `ForIteratorFact { declaration, element_type, source }` with
  `ForIterableSource::{Range, Array { element_managed: ManagedReferenceKind }}`.
  `for_iterator_fact_tracked` proves arrays through `generic_source_expression_identity` of the
  iterable (`GenericSourceTypeIdentity::Array(element)`), so a parameter, an annotated or inferred
  local, a field projection, a call result or an index into an array of arrays all qualify, and an
  erased pointer never does. The loop variable's typing follows the same identity:
  `local_declaration_type` (syntactic, range and declared-array locals),
  `generic_source_local_identity` and `abi_local_declaration_type` (the `ForStatement` arm). The
  check gate (`gate/control.rs`) accepts a `for` whose fact exists, unchanged.
- `beskid_isle`: `ForIterableKind::Array`; `NodeFacts::array_for_fact(key) -> Option<ArrayForFact
  { layout: ArrayLayout, element_managed: ManagedReferenceFact }>`; `emit_array_for` lowers the
  loop: evaluate the array, trap on null, root it for the loop (a managed expression root in the
  enclosing scope), load the length once, bind the loop variable (a scalar or a GC-rooted pointer
  local), and per iteration reload the data pointer, bounds are implied by the header, load the
  element with the layout stride, assign the local (updating its root slot), lower the body in its
  own root scope, increment. `emit_iterator_for` (nominal iterator protocol) keeps failing closed
  with its own error kind.
- `beskid_codegen`: `array_for_fact_impl` maps the fact to the ISLE layout
  (`scalar_aware_array_layout`), with the enclosing generic specialization applied through
  `for_array_element_specialization` when the element names a generic parameter.

### 3.3 The false "unused private item `Main`" warning

`check_unused_private_items` exempted the lowercase name `main`, which is not the Beskid entry
point (`beskid run --entrypoint` defaults to `Main`; `native_link_entrypoint` maps `Main` to the
C `main` symbol). Fix: one constant `beskid_analysis::projects::PROGRAM_ENTRY_FUNCTION = "Main"`
used by the CLI defaults and `beskid_aot::api::DEFAULT_ENTRYPOINT`;
`AnalysisOptions::program_entry_function` is set by the spine for the entry unit of an `App` or
`Test` target (never for sibling units or entry-less libraries), and the rule exempts exactly that
name in that unit. The lowercase exemption is removed.

## 4. Migration steps (each a commit on this branch)

1. This document.
2. Retire the checker: `ExecutableFrontEnd`, spine without the `LOWER` typing block, authority
   required on both paths, deletions listed in section 2, `DependencyTypingPolicy` removal,
   the dead `type_descriptors` path, phase ids, project-less check through the query gate, and the
   test ports listed in section 5.
3. `for` over arrays (queries, ISLE, codegen, tests, coverage table).
4. The entry-function constant and the warning fix.
5. OpenSpec change `v06-query-front-end` (staged), changelog.

## 5. Tests

Removed with the checker: everything under `beskid_analysis/src/types/`, the five
`beskid_analysis/tests/*` files that drove `TypeChecker` or `resolve_and_type_program`
(`string_ordering_v06`, `try_desugar_targets`, `unqualified_receiver_members_v06`,
`v06_same_leaf_signature`, `v06_dependency_call_argument_types`), `services/runtime_types_tests.rs`.

Ported to the query authority:

- `beskid_queries/src/entry/parity_front_end.rs` (parity harness): string ordering, runtime
  primitive signatures, module integer constants, same-leaf dependency signatures, dependency call
  argument types, unknown bare call inside a method, missing accessible field (E1212).
- `beskid_queries/tests/v06_front_end_authority.rs`: bare sibling-method dispatch through
  `call_lowering`, `try_expression_fact` for `let`-bound operands, spine products (`executable`
  always present, syntax assembly projection, editor recovery policy).
- `beskid_codegen/tests/prepared_syntax.rs`: named function value, function-typed field, call on a
  call result and `for` over an array lower through the prepared front end.
- `beskid_codegen/tests/isle_adapter/control_flow_loop_termination.rs`: CLIF shape of the array
  loop (scalar and managed elements, break inside the body).
- `beskid_tests_projects` fixture harness and `beskid_queries/tests/runtime_fixture.rs`: the
  "production semantic gate" assertions use `semantic_diagnostics_for_roots`; the resolution-table
  assertions use the resolver directly.
- `beskid_analysis` visibility rule: the entry function is not an unused private item; `main` is.

## 6. Verification (coordinator, cwd `compiler/`)

```
cargo check --locked --workspace --all-targets
cargo test --locked -p beskid_analysis
cargo test --locked -p beskid_queries
cargo test --locked -p beskid_isle
cargo test --locked -p beskid_codegen
cargo test --locked -p beskid_lsp
cargo test --locked -p beskid_cli
cargo test --locked -p beskid_tests_projects
cargo test --locked -p beskid_engine --test native_kit
```

Then the Corelib check gate and the fixture matrix as in `.build/rc-fast.sh`.
