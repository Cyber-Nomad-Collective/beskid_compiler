# Beskid ISLE Lowering: Coverage and Completion Plan

Status: living document. Generated from a scan of `crates/beskid_isle` and the reference
lowering surface in `crates/beskid_codegen`.

## Purpose

Beskid is migrating instruction selection off the legacy HIR lowering path onto a generated
**ISLE** ("Instruction Selection Lowering Expressions") path. This document inventories:

1. Which lowering rules are **implemented** today in ISLE, and in which file each lives.
2. Which rules remain **to implement** to fully complete Beskid lowering (i.e. reach parity
   with the language surface that the legacy `beskid_codegen` path can lower), and where each
   new rule should go.

Use the two tables in [Implemented rules](#implemented-isle-rule-files) and
[Rules to implement](#rules-to-implement) as the working checklist.

## How the ISLE lowering path fits together

The ISLE path selects stock Cranelift CLIF for each Beskid AST node. It has three layers:

- **Rules (`crates/beskid_isle/isle/*.isle`).** Declarative `(rule ...)` clauses that match an
  `AstNodeKey` by facts (`node_kind`, `operator_fact`, `call_kind`, `literal_kind`,
  `index_target`, ...) and select an emitter. `lower_expression` returns a `Value`;
  `lower_statement` returns `Unit`. `build.rs` compiles these to generated Rust.
- **Emitter glue (`crates/beskid_isle/src/*.rs`).** `IsleContext` implements every `extern
  constructor`/`extractor` the rules call, driving a Cranelift `FunctionBuilder`. It reads all
  structural facts through the `NodeFacts` trait, and reports selection gaps as a
  `LoweringError` whose kind is usually `MissingRuleOrFact`.
- **Fact adapter (`crates/beskid_codegen/src/isle_adapter.rs`).** `SyntaxNodeFacts` answers
  `NodeFacts` from generation-safe Salsa/syntax queries. Facts that are not yet ported simply
  return `None`, so an unported construct surfaces deterministically as `MissingRuleOrFact`
  instead of falling back to HIR.

A construct is "fully lowered by ISLE" only when all three exist: a matching rule, an emitter
in `IsleContext`, and the facts feeding it in `SyntaxNodeFacts`.

## Node surface reference

The AST surface ISLE must eventually cover is enumerated by `NodeKind` in
`crates/beskid_isle/isle/types.isle`, cross-checked against the legacy dispatchers
`crates/beskid_codegen/src/lowering/expressions/expression.rs` and
`.../statements/statement.rs`. Some HIR nodes (`MacroInvocation`, `MacroMetavariable`,
`CodeStringExpression`, raw `TryExpression`) are rejected or desugared before codegen in both
paths and are out of scope for ISLE (see [Out of scope](#out-of-scope)).

## Implemented ISLE rule files

Rule files live in `crates/beskid_isle/isle/`. Every rule delegates to an emitter implemented
in `crates/beskid_isle/src/lib.rs` (`IsleContext`), with primitives in `src/context.rs` and
dispatch-route emission in `src/dispatch.rs`.

| File | Kind | Implemented lowering rules / constructs |
| --- | --- | --- |
| `types.isle` | Foundation | Type + enum declarations shared by all rules: `AstNodeKey`, `Value`, `Unit`, `StatementCursor`, and the fact enums `NodeKind`, `CursorKind`, `LiteralKind`, `OperatorFact`, `CallKind`, `IndexTarget`. No rules. |
| `ast.isle` | Foundation | Fact extractors (`node_kind`, `literal_kind`, `operator_fact`, `call_kind`, `assignment_target_kind`, `for_iterable_kind`) and the `child_at` child-access constructor. No rules. |
| `primitives.isle` | Foundation | Trusted CLIF primitive constructors: `iconst_i64`, `load_i64`/`store_i64`, `load_i8_zext`, `ptr_add`, `icmp_eq`/`icmp_ne`/`icmp_slt`, `icmp_byte_ne`, `bounded_memcmp`. No rules. |
| `expressions.isle` | Expressions | `lower_expression` entry decl; `GroupedExpression` (unwrap child 0); `BlockExpression` value (`emit_block_expression`). |
| `literals.isle` | Expressions | Literal lowering for `Integer`, `Boolean`, `Float`, `Char`, `String`. |
| `binary.isle` | Expressions | Binary arithmetic/comparison: `IdentityEq`, `IdentityNotEq`, `Eq`, `NotEq`, `Lt`, `Lte`, `Gt`, `Gte`, `Add`, `Sub`, `Mul`, `Div`, `Mod` (integer CLIF `iadd`/`isub`/`imul`/`sdiv`/`srem`/`icmp`). |
| `unary_casts.isle` | Expressions | Unary `Neg` (`ineg`) and `Not` (`bnot`). |
| `control_flow.isle` | Expr + Stmt | Short-circuit `Or`/`And`; `IfStatement` (`emit_if_else`); `WhileStatement`; `BreakStatement`; `ContinueStatement`; `ForStatement` over a `RangeExpression` iterable (`emit_range_for`). |
| `calls.isle` | Expressions | Direct call expressions (`CallKind.Direct` -> `emit_direct_call`), including method/receiver calls resolved to a direct callee. |
| `runtime_intrinsics.isle` | Expressions | Canonical runtime-intrinsic calls (`CallKind.RuntimeIntrinsic` -> `emit_runtime_intrinsic`). |
| `dispatch.isle` | Expressions | Dynamic dispatch calls (`CallKind.Dynamic`); string `concat`/`eq`/`ne` from interpolation desugar (`StringAdd`/`StringEq`/`StringNotEq`); `string[index]` byte read (`IndexTarget.String`). |
| `memory.isle` | Expressions | `PathExpression` local read; `AssignExpression` to a `PathExpression` target (local assign) and to a `FieldExpression` target (field assign); `ArrayLiteralExpression`; `IndexExpression` array read (`IndexTarget.Array`); `StructLiteralExpression`; `FieldExpression` read; `EnumLiteralExpression` (nullary + single-payload); `MatchExpression` (enum-tag switch). |
| `statements.isle` | Statements | `ExpressionStatement`; `ReturnStatement`; `LetStatement`; `BlockExpression`/`TestDefinition` statement-cursor traversal; statement sequencing (`sequence_statements` / `finish_statements`). |
| `items.isle` | Items | `item_body` selection for `FunctionDefinition` (child 0) and `TestDefinition` (self). |

### Supporting Rust (not ISLE rules, but part of the lowering path)

| File | Role |
| --- | --- |
| `crates/beskid_isle/src/lib.rs` | `IsleContext` emitter implementations for every extern constructor/extractor; `FunctionEmitter` entrypoints (`emit_expression`, `emit_statement`, `emit_item_statement*`); `NodeFacts` trait; `LoweringError` / `LoweringErrorKind` (`MissingRuleOrFact`, layout/match errors). |
| `crates/beskid_isle/src/context.rs` | Bridges the trusted CLIF primitives in `primitives.isle` to `FunctionBuilder`. |
| `crates/beskid_isle/src/dispatch.rs` | Emits dispatch-route calls (`emit_dispatch_call`, `emit_str_from_i64_dispatch`) used by dynamic + string rules. |
| `crates/beskid_isle/build.rs` | Compiles `isle/*.isle` into the generated selector consumed by `IsleContext`. |
| `crates/beskid_codegen/src/isle_adapter.rs` | `SyntaxNodeFacts` (query-backed `NodeFacts`) plus `emit_isle_expression` / `emit_isle_item*` production entrypoints and ABI/layout derivation. |
| `crates/beskid_codegen/src/lowering/dispatch.rs` | Thin bridge from codegen call sites to `beskid_isle::emit_dispatch_call`. |
| `crates/beskid_isle/src/isle` tests (`crates/beskid_isle/tests/*`) | Rule/emitter coverage (`rule_coverage.rs` asserts each owned group has a real rule and every operator fact has a rule). |

## Rules to implement

These are constructs the reference (legacy `beskid_codegen`) path lowers, or language features
required for full parity, that ISLE cannot yet select. Each currently fails as
`MissingRuleOrFact` (or is silently unreachable because no `NodeKind` variant exists). "Target
file" is where the new rule belongs; a new `NodeKind`/`OperatorFact` variant in `types.isle`
plus a `SyntaxNodeFacts` fact in `isle_adapter.rs` and an emitter in `lib.rs` are implied for
each unless noted.

| # | Construct / rule to add | Target ISLE file | Legacy reference | Notes / scope |
| --- | --- | --- | --- | --- |
| 1 | `SpawnExpression` lowering | new `spawn.isle` | `expressions/spawn_expression.rs` | Concurrency: spawn a lambda target as a fiber; needs capture-env materialization. No `NodeKind::SpawnExpression` yet. |
| 2 | `LambdaExpression` / closure values | new `lambda.isle` | `expressions/call_expression.rs::lower_lambda_function_value` | Lambda function value + capture environment struct. No `NodeKind` variant; also unblocks (1). |
| 3 | `LaunchStatement` lowering | new `composition.isle` | `lowering/composition/launch_statement.rs` | Emit `composition_container_create` -> `composition_launch` -> body -> `composition_shutdown` -> `composition_container_drop`. No `NodeKind` variant. |
| 4 | `WithStatement` lowering | new `composition.isle` | `lowering/composition/with_statement.rs` | Emit `composition_scope_enter` / `composition_scope_leave` brackets around body. No `NodeKind` variant. |
| 5 | Index-target assignment `arr[i] = v` | `memory.isle` (+ `dispatch.isle` for string bytes) | `expressions/assign_expression.rs` (`AssignTargetKind::IndexElement`) | Bounds-checked array element store + GC write barrier for pointer-like elements; string byte write. ISLE currently handles only `PathExpression` and `FieldExpression` assign targets. |
| 6 | Compound assignment `+=` / `-=` | `memory.isle` | `expressions/assign_expression.rs` (`AddAssign`/`SubAssign`) | For local, field, and index targets; includes float `fadd`/`fsub`, integer `iadd`/`isub`, and string `+=` concat. Needs compound-assign `OperatorFact`s (currently absent). |
| 7 | Event-member `+=` / `-=` (subscribe/unsubscribe) | `dispatch.isle` | `expressions/assign_expression.rs` (`AssignTargetKind::EventMember`) | Lower to `TAG_EVENT_SUBSCRIBE` / `TAG_EVENT_UNSUBSCRIBE_FIRST` dispatch routes with capacity. Depends on event-field facts. |
| 8 | Full `match` patterns | `memory.isle` (extend `emit_match`) | HIR match lowering | Current `emit_match` only switches on an enum tag to arm bodies. Missing: payload binding into arm locals, literal/struct/nested patterns, and guards. |
| 9 | `ForStatement` over array/collection iterables | `control_flow.isle` | (none; legacy `ForStatement` is `UnsupportedNode`) | `for x in arr { ... }`. Needed for full completion; neither path implements it today. Range-for is already done. |
| 10 | Non-field member reads | `memory.isle` | `expressions/member_expression.rs` | Fiber-handle `.handle` passthrough and other special member reads not backed by an `aggregate_field_access` layout fact. General field reads on expression receivers already work via recursive base lowering. |
| 11 | Multi-segment path field-chain assignment | `memory.isle` | `expressions/assign_expression.rs` (`load_path_field_chain`) | Assignment through a chained path receiver (`a.b.c = v`) beyond a single field level. Confirm coverage; extend `emit_field_assign` base resolution if the chain fact is unavailable. |

### Suggested ordering

1. **#5, #6, #8** close everyday imperative gaps (index writes, compound assignment, real match
   patterns) that block ordinary application code on the syntax-only path.
2. **#2 then #1** (lambda before spawn, since spawn targets are lambdas).
3. **#3, #4, #7** cover the composition/eventing surface; gated today by
   `composition_policy::RUNTIME_CONTAINER_LOWERING_ENABLED`.
4. **#9, #10, #11** are smaller parity items.

## Out of scope

These reach codegen only in error or are desugared upstream, and are intentionally not ISLE
lowering targets:

- `MacroInvocation`, `MacroMetavariable` (expanded before codegen).
- `CodeStringExpression` (unsupported in both paths).
- Raw `TryExpression` (normalized to `match` before codegen).

## How to verify a newly added rule

1. Add the `NodeKind`/`OperatorFact` variant (if new) to `crates/beskid_isle/isle/types.isle`.
2. Add the `(rule ...)` to the appropriate rule file and the emitter to
   `crates/beskid_isle/src/lib.rs` (`IsleContext`).
3. Supply the facts in `crates/beskid_codegen/src/isle_adapter.rs` (`SyntaxNodeFacts`).
4. Extend `crates/beskid_isle/tests/rule_coverage.rs` and add a focused emitter test under
   `crates/beskid_isle/tests/` mirroring the existing per-construct tests.
5. Trace failures with `BESKID_COMPILER_TRACE=1`, which records selection failures and CLIF
   emission per source key.
