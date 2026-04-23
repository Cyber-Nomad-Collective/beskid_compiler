# Formatter test matrix

Maps **Emit** coverage to **fixtures** under [`crates/beskid_tests/fixtures/format/`](../crates/beskid_tests/fixtures/format/) (recursive subdirectories) and **Rust tests** in [`crates/beskid_tests/src/format/mod.rs`](../crates/beskid_tests/src/format/mod.rs). Every row should have at least one **golden** (fixture or `#[test]`) plus **idempotence** (fixture harness or test).

Legend: **F:** fixture path (`*.input.bd`), **T:** inline test name.

## Expressions (`Expression` enum)

| Variant | Covered by | Notes |
| --- | --- | --- |
| Match | F `core`, F `docs_and_control`, F `expr/expr_match_guard`, T `format_type_enum_contract_match` | Empty match: F `edges/edges_empty_match` |
| Lambda | F `expr/expr_lambda_assign_block` | Unary + multi-param |
| Assign | F `expr/expr_lambda_assign_block` | `=` on inferred `let` binding |
| Binary | F `expr/expr_binary_ops`, T `format_binary_ops_emit_canonical_operators` | `<=` / `>=` in `let` assignments are lexer-ambiguous; covered in table test only for operators that parse in that position |
| Unary | F `expr/expr_unary_literals` | `!`, `-` |
| Call | F `expr/expr_binary_ops`, F `expr/expr_call_member_path` | |
| Member | F `expr/expr_call_member_path` | |
| Literal | F `expr/expr_unary_literals` | bool, int, string |
| Path | F `expr/expr_call_member_path`, F `core` | |
| StructLiteral | F `expr/expr_struct_enum_try`, F `expr/expr_call_member_path` | |
| EnumConstructor | F `expr/expr_struct_enum_try`, F `core` | |
| Block | F `expr/expr_lambda_assign_block` | Block expression in `let` |
| Grouped | T `format_if_condition_grouped_idempotent` | |
| Try | F `expr/expr_struct_enum_try` | Postfix `?` |

## Statements (`Statement` enum)

| Variant | Covered by | Notes |
| --- | --- | --- |
| Let (untyped) | F `core`, T `format_if_while...` | |
| Let (typed) | F `stmt/stmt_control_flow` | `i32 mut x = …` |
| Return | Many fixtures | |
| Break / Continue | T `format_if_while...`, F `stmt/stmt_control_flow` | |
| While | T `format_if_while...`, F `docs_and_control` | |
| For | F `stmt/stmt_control_flow` | |
| If (+ else) | F `docs_and_control`, F `stmt/stmt_control_flow` | |
| Expression | F `stmt/stmt_control_flow` | Implied via control flow bodies |

## Top-level items (`Node` enum)

| Variant | Covered by | Notes |
| --- | --- | --- |
| Function | F `core`, F `expr/*`, etc. | |
| Method | F `items/items_types_contracts` (from `impl`, pre-format AST) | After format, methods from `impl` may re-parse as **functions**; harness treats `method` vs `function` as equivalent for top-level shape checks |
| TypeDefinition | F `core`, F `items/*` | |
| EnumDefinition | F `core`, F `items/items_types_contracts`, F `expr/expr_match_guard` | |
| ContractDefinition | T `format_type_enum_contract_match`, F `items/items_types_contracts` | |
| AttributeDeclaration | F `items/items_attribute_decl` | |
| ModuleDeclaration | F `items/items_mod_use_attr` | |
| InlineModule | F `docs_and_control`, F `items/items_mod_use_attr` | |
| UseDeclaration | F `core`, F `items/items_mod_use_attr` | |
| TestDefinition | F `tests` | `tests.input.bd` |

## Patterns and match arms

| Case | Covered by |
| --- | --- |
| Wildcard / enum pattern | F `core`, F `expr/expr_match_guard` |
| Match guard (`when`) | F `expr/expr_match_guard` | Uses `Foo::Bar when 1 == 1` |

## Leading docs (`///`)

| Case | Covered by |
| --- | --- |
| Module + function | F `docs_and_control` |
| Multi-line file docs | F `items/items_leading_docs` | |

## Layout policy (`policy.rs`)

| Hook | Covered by |
| --- | --- |
| `between_top_level_declarations` | F `core`, T `format_golden_use_and_function` |
| `between_members` | F `core`, T `format_type_enum_contract_match` |
| `between_block_items` | T `format_if_while...`, F `stmt/stmt_control_flow` |

## Structural `ImplBlock` (not `format_program`)

| Case | Covered by |
| --- | --- |
| Pest fragment + `Emitter` | T `impl_block_emitter_works_for_structural_ast` |

Program-level `impl` is expanded to `Node::Method` items before formatting; canonical output uses function-shaped items.

## Harness invariants (all fixtures)

- Golden text equals `format_program(parse(input))`.
- Idempotence: `format(parse(format(parse(x)))) == format(parse(x))`.
- Top-level `Node` kind sequence: compared with **relaxed** mapping (`method` and `function` treated as the same bucket) so `impl`-sourced methods round-trip through the formatter.
- **Statement count** parity: recursive count of statements in function / method / test / inline-module bodies (including nested control-flow blocks and block/match bodies in `let` / `return` / expression statements) must match before vs after format.

## LSP parity

- T `formatting_matches_format_program_fixture_docs_and_control` in [`beskid_lsp`](../crates/beskid_lsp/src/features/formatting/handler.rs) asserts `handle_document_formatting` output equals `format_program` on the `docs_and_control` fixture.

## Blessing expected files

From repo `compiler/` root:

```bash
cargo build -p beskid_cli
python3 scripts/bless_format_fixtures.py
```

## Optional corelib corpus

When `BESKID_FORMAT_CORPUS=1`:

```bash
python -m nox --non-interactive -s format_corpus_corelib
```

Runs `scripts/format_corpus_check.py` (`beskid format --check` on each `corelib/beskid_corelib/**/*.bd`).
