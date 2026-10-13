# Beskid ISLE Lowering: Coverage and Completion Plan

Status: living document, updated for 0.6.0 (in qualification) from a scan of
`crates/beskid_isle/isle`.

## Purpose

ISLE ("Instruction Selection Lowering Expressions") is the only lowering path. In 0.6 the query
gate's executable front end is lowered through `TypedProgram`, `CodegenInput` and ISLE to stock
Cranelift CLIF; the legacy HIR lowering and the `beskid_codegen/src/lowering/` tree are deleted.
The "Legacy reference" column below names those deleted files for history only. This document
inventories:

1. Which lowering rules are **implemented** today in ISLE, and in which file each lives.
2. Which constructs the check gate accepts but ISLE still cannot select, and where each new rule
   should go.

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
- **Fact adapter (`crates/beskid_codegen/src/isle_adapter/`).** `SyntaxNodeFacts` answers
  `NodeFacts` from generation-safe Salsa/syntax queries. Facts that are not yet ported simply
  return `None`, so an unported construct surfaces deterministically as `MissingRuleOrFact`;
  there is no fallback lowering path.

A construct is "fully lowered by ISLE" only when all three exist: a matching rule, an emitter
in `IsleContext`, and the facts feeding it in `SyntaxNodeFacts`.

## Node surface reference

The AST surface ISLE must cover is enumerated by `NodeKind` in
`crates/beskid_isle/isle/types.isle`. Some syntax nodes (`MacroInvocation`, `MacroMetavariable`,
`CodeStringExpression`) are rejected or expanded before codegen and are out of scope for ISLE (see
[Out of scope](#out-of-scope)).

## Implemented ISLE rule files

Rule files live in `crates/beskid_isle/isle/`. Every rule delegates to an emitter implemented
in `crates/beskid_isle/src/context.rs` and `src/context/` (`IsleContext`), with trusted primitives in
`src/clif_primitives.rs` and dispatch-route emission in `src/dispatch.rs`.

| File | Kind | Implemented lowering rules / constructs |
| --- | --- | --- |
| `types.isle` | Foundation | Type + enum declarations shared by all rules: `AstNodeKey`, `Value`, `Unit`, `StatementCursor`, and the fact enums `NodeKind`, `CursorKind`, `LiteralKind`, `OperatorFact`, `CallKind`, `IndexTarget`. No rules. |
| `ast.isle` | Foundation | Fact extractors (`node_kind`, `literal_kind`, `operator_fact`, `call_kind`, `assignment_target_kind`, `for_iterable_kind`) and the `child_at` child-access constructor. No rules. |
| `primitives.isle` | Foundation | Trusted CLIF primitive constructors (contract; Rust bridge in `clif_primitives.rs`): extending loads (`load_i8/i16/i32` zext/sext), stores, `icmp_*` including `ult`, f64 arith/`fcvt_*`, `icmp_byte_ne`, `bounded_memcmp`. Explicit `stack_load`/`stack_store` live on `ClifPrimitives` for scratch slots (AST locals use SSA vars). No AST rules. |
| `expressions.isle` | Expressions | `lower_expression` entry decl; `GroupedExpression` (unwrap child 0); `BlockExpression` value (`emit_block_expression`); `SpawnExpression` (`emit_spawn`); `LambdaExpression` (`emit_lambda`, closure record); `TryExpression` as expression and statement; `ClifBlock`. |
| `literals.isle` | Expressions | Literal lowering for `Integer`, `Boolean`, `Float`, `Char`, `String`. |
| `binary.isle` | Expressions | Binary arithmetic/comparison: `IdentityEq`, `IdentityNotEq`, `Eq`, `NotEq`, `Lt`, `Lte`, `Gt`, `Gte`, `Add`, `Sub`, `Mul`, `Div`, `Mod`. Emitters are type-polymorphic: float uses `fadd`/`fsub`/`fmul`/`fdiv`/`fcmp`; CLIF `i8` (Beskid `u8`/`bool`) uses unsigned compares and `udiv`/`urem`; wider integers stay signed. Float `%` fails closed (partial `clif_srem`). |
| `unary_casts.isle` | Expressions | Unary `Neg` (`ineg`/`fneg`) and `Not` (`bnot`). |
| `control_flow.isle` | Expr + Stmt | Short-circuit `Or`/`And`; `IfStatement` (`emit_if_else`); `WhileStatement`; `BreakStatement`; `ContinueStatement`; `ForStatement` over a `RangeExpression` iterable (`emit_range_for`), over an array value (`emit_array_for`, selected by `array_for_fact`), and over a nominal iterator whose `Next()` returns `Option<T>` (`emit_iterator_for`; the iterable is rooted and `Next()` resolves per specialization). |
| `calls.isle` | Expressions | Call expressions by `CallKind`: `Direct` (including method/receiver calls resolved to a direct callee), `FunctionValue` (indirect call through a closure record), `InlineLambda` (stack closure called inline), `Bulk`, `TypedArrayAllocation`, `PrimitiveNumericConversion`, `CollectionOperation`. |
| `events.isle` | Expr + Stmt | Event subscribe and unsubscribe-first assignments; event raise statements. |
| `composition.isle` | Statements | `LaunchStatement` and `WithStatement`, selected only for a source key the frozen composition graph owns. |
| `runtime_intrinsics.isle` | Expressions | Canonical runtime-intrinsic calls (`CallKind.RuntimeIntrinsic` -> `emit_runtime_intrinsic`). |
| `dispatch.isle` | Expressions | Dynamic dispatch calls (`CallKind.Dynamic`); string `concat`/`eq`/`ne` from interpolation desugar (`StringAdd`/`StringEq`/`StringNotEq`); `string[index]` byte read (`IndexTarget.String`). |
| `memory.isle` | Expressions | `PathExpression` local read; `AssignExpression` to a `PathExpression` target (local assign), to a `FieldExpression` target (field assign), and to an `IndexExpression` target (`emit_index_assign`); `ArrayLiteralExpression`; `IndexExpression` array read (`IndexTarget.Array`); `StructLiteralExpression`; `FieldExpression` read; `EnumLiteralExpression` (nullary + single-payload); `MatchExpression` (`emit_match` / `emit_match_statement`, see [Match coverage](#match-coverage)). |
| `statements.isle` | Statements | `ExpressionStatement` (including direct, function-value and runtime-intrinsic call statements); `ReturnStatement`; `LetStatement`; `ScopedUseStatement`; `BlockExpression`/`TestDefinition`/`MethodDefinition` statement-cursor traversal; statement sequencing (`sequence_statements` / `finish_statements`). |
| `items.isle` | Items | `item_body` selection for `FunctionDefinition` (child 0) and `TestDefinition` (self). |

### Supporting Rust (not ISLE rules, but part of the lowering path)

| File | Role |
| --- | --- |
| `crates/beskid_isle/src/context.rs`, `src/context/` | `IsleContext` emitter implementations for every extern constructor/extractor. |
| `crates/beskid_isle/src/facts/`, `src/errors.rs`, `src/emitter.rs` | `NodeFacts` trait; `LoweringError` / `LoweringErrorKind`; `FunctionEmitter` entrypoints. |
| `crates/beskid_isle/src/clif_primitives.rs` | Bridges the trusted CLIF primitives in `primitives.isle` to `FunctionBuilder` (plus `stack_load`/`stack_store` scratch helpers). |
| `crates/beskid_isle/src/dispatch.rs` | Emits dispatch-route calls (`emit_dispatch_call`, `emit_str_from_i64_dispatch`) used by dynamic + string rules. |
| `crates/beskid_isle/build.rs` | Compiles `isle/*.isle` into the generated selector consumed by `IsleContext`. |
| `crates/beskid_codegen/src/isle_adapter/` | `SyntaxNodeFacts` (query-backed `NodeFacts`) plus `emit_isle_expression` / `emit_isle_item*` production entrypoints and ABI/layout derivation. |
| `crates/beskid_isle/src/isle` tests (`crates/beskid_isle/tests/*`) | Rule/emitter coverage (`rule_coverage.rs` asserts each owned group has a real rule and every operator fact has a rule). |

### Match coverage

`emit_match` (value position) and `emit_match_statement` lower every `match` form the 0.6 check
gate admits, as an ordered chain of arm tests (`crates/beskid_isle/src/context/enums.rs`). The
facts are `enum_match` / `enum_match_specialization` / `enum_match_call_specialization` for an
enum scrutinee and `scalar_match` for a primitive one, both in
`crates/beskid_queries/src/semantic_contract/layouts/enum_layout/match_materialize.rs`.

| Form | Fact / emitter |
| --- | --- |
| Enum scrutinee: local, parameter, call, member, index, projected field, or a pattern binding of an enclosing match | `enum_match`. A binding of a generic payload (`Result::Ok(sent) => match sent`, `sent : Option<i64>`) takes the binding's applied source identity as its layout authority. |
| Unit, single-field, and multi-field variants; wildcard; payload bindings | `EnumMatchPatternFact::{Enum, Wildcard, Binding}` -> tag test, then one field test per payload slot. |
| Nested enum patterns, including a generic payload (`Result::Ok(Option::Some(x))`) | Recursive `Enum` pattern; the nested layout comes from the payload's applied identity. |
| Literal patterns in payloads and on primitive scrutinees: integer (an unsuffixed literal takes the expected integer type), float, `bool`, `char`, `string` | `ScalarLiteral` -> `icmp`/`fcmp`; `StringLiteral` -> `str_eq`. |
| Whole-scrutinee binding arm (`other => ...`) on enum and primitive scrutinees | `MatchArmFact::value` with a `Binding` pattern. |
| `when` guards | `EnumMatchArmFact::guard` -> evaluated after the bindings are installed; a false guard releases the arm's roots and continues with the next arm. Guarded arms do not count toward exhaustiveness. |
| Diverging arms in value position (`return`, `break`, `continue`, a `never` call, a block that cannot fall through) | The arm's type is `never`; it terminates without a merge value. |
| Match as a statement; arms as unit calls, empty blocks, or blocks with locals, nested control flow, and nested matches | `emit_match_statement`. A primitive statement match without a catch-all arm continues after the match when no arm applies. |

Exhaustiveness: an enum match without an unguarded catch-all arm must cover every variant with
unguarded arms whose payload patterns are collectively irrefutable. The legality gate
(`match_exhaustiveness`, E1304) applies the same judgement to the `enum_match` fact and names one
uncovered value pattern (`Result::Ok(_)`), so `check` rejects exactly what `emit_match_dispatch`
would reject as `NonExhaustiveMatch`. A value-position primitive match without a catch-all arm
traps when no arm applies.

A binding of a zero-sized `unit` payload (`Result::Ok(value)` over `Result<unit, E>`) has no
storage. Read as a value it is the unit constant: call arguments, enum payloads, aggregate fields
and `return` of a `unit` function evaluate it for effect only.

## Rules to implement

These are constructs that ISLE cannot yet select. Each fails as `MissingRuleOrFact` or another
`LoweringError`. "Target file" is where the new rule belongs; a new `NodeKind`/`OperatorFact`
variant in `types.isle`, a `SyntaxNodeFacts` fact in `beskid_codegen/src/isle_adapter/` and an emitter in the
`beskid_isle` context are implied for each unless noted.

Spawn, lambdas and closure values, function-value calls, index assignment and event
subscribe/unsubscribe/raise now have rules (see the table above) and are no longer listed.

| # | Construct / rule to add | Target ISLE file | Notes / scope |
| --- | --- | --- | --- |
| 1 | `LaunchStatement` registration and injection completion | `composition.isle` | The rule exists. The 0.5 note recorded that ordered registration construction, injection and startup remained release work; this has not been re-verified for 0.6. |
| 2 | `WithStatement` scope identity completion | `composition.isle` | The rule exists. The 0.5 note recorded that the runtime ABI still needed to carry the validated scope ID; this has not been re-verified for 0.6. |
| 3 | Compound assignment `+=` / `-=` on values | `memory.isle` | For local, field and index targets; includes float `fadd`/`fsub`, integer `iadd`/`isub` and string `+=` concat. No compound-assign rule exists; event `+=`/`-=` is handled by `events.isle`. |
| 6 | Non-field member reads | `memory.isle` | Fiber-handle `.handle` passthrough and other special member reads not backed by an `aggregate_field_access` layout fact. |
| 7 | Multi-segment path field-chain assignment | `memory.isle` | Assignment through a chained path receiver (`a.b.c = v`) beyond one field level. Confirm coverage; extend `emit_field_assign` base resolution if the chain fact is unavailable. |

## Out of scope

These reach codegen only in error or are desugared upstream, and are intentionally not ISLE
lowering targets:

- `MacroInvocation`, `MacroMetavariable` (expanded before codegen).
- `CodeStringExpression` (unsupported).
- SIMD vector ops (Beskid has no vector types in TypedProgram today).
- Bitwise / shift / rotate / bitcount operators (no Beskid language operators).
- Cranelift `vmctx` parameters (managed ABI uses `GlobalValueData::Symbol` + runtime helpers).
- Direct `br_table` ISLE rules (`emit_match` already emits via `cranelift_frontend::Switch`).

## How to verify a newly added rule

1. Add the `NodeKind`/`OperatorFact` variant (if new) to `crates/beskid_isle/isle/types.isle`.
2. Add the `(rule ...)` to the appropriate rule file and the emitter to
   `crates/beskid_isle/src/context/` (`IsleContext`).
3. Supply the facts in `crates/beskid_codegen/src/isle_adapter/` (`SyntaxNodeFacts`).
4. Extend `crates/beskid_isle/tests/rule_coverage.rs` and add a focused emitter test under
   `crates/beskid_isle/tests/` mirroring the existing per-construct tests.
5. Trace failures with `BESKID_COMPILER_TRACE=1`, which records selection failures and CLIF
   emission per source key.
