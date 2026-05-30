# corelib_compiler_sdk

Typed Mod SDK facades under `Beskid.Syntax` and `Beskid.Compiler.*` for compiler mod projects.

## Generated surfaces

`src/Beskid/Compiler/{Query,Diagnostics,TypedEmitter,Compilation}.bd` each combine:

1. **Generated** structural mirrors from `beskid_analysis` (via `beskid_ast_reflect_gen`), using `ReflectStub` for payloads where the Rust shape is not mapped field-for-field.
2. **Hand-maintained** version tokens and facade helpers at the bottom of each file.

`src/Beskid/Compiler/Collect.bd` is hand-maintained and declares the `Collector`, `Generator`, `Analyzer`, `Rewriter`, and `AttributeGenerator` contracts discovered from AOT-compiled `Mod` packages.

**Syntax** is split for stack-friendly consumption:

- `src/Beskid/Syntax.bd` — thin entry (`pub mod …Nodes`, `SyntaxFacadeVersion`); no `ReflectStub` here (other hubs like `Query.bd` may still define a local stub for stitched enums).
- `src/Beskid/Syntax/Nodes.bd` — barrel that re-exports one module per AST node type and per generated list/optional helper.
- `src/Beskid/Syntax/Nodes/*.bd` — one file per logical syntax node (from `beskid_analysis` `syntax/**` scan), with concrete types where the mapping is straightforward (`string`, `bool`, `i64`, cross-references under `Beskid.Syntax.Nodes.*`), plus `{T}List` / `Optional{T}` helpers for typical `Vec` / `Option` shapes (`Spanned<T>` is peeled to `T` for helper naming). Remaining opaque Rust shapes still collapse to a placeholder type string inside the generator, but checked-in syntax nodes aim to be stub-free.

### Syntax node field names (generated)

Regeneration uses `beskid_ast_reflect_gen::syntax_nodes::emit_syntax_sdk` (see `crates/beskid_ast_reflect_gen/README.md`).

- **Struct fields:** Beskid field names use **lowerCamel** from Rust `snake_case` (with keyword escapes); tuple-struct Rust fields use `field_0`, `field_1`, … (never `f0` / `f1`).
- **Enum tuple variants:** a single payload uses `payload`; multiple unnamed slots use `variant_field_0`, `variant_field_1`, …
- **Beskid keywords / keyword-prefix clashes:** names that match a Beskid reserved word, or start with one as the first snake segment (e.g. `contract_name`, `type_name`), are escaped as `_name` so they parse as normal identifiers.
- **Documentation:** each node file opens with a structured `///` header (mirror path, optional multi-line Rust docs, a generated index using `@arg` / `@variant` per `beskid_doc.pest`, and `ReflectStub` callouts as `@arg` where needed). Each field and enum variant has its own `///` run (Rust docs when present, plus directive lines). List/optional helpers use the same `@variant` / `@arg` style.
- `src/Beskid/Syntax/Nodes/_inventory.txt` — sorted type names (checked in tests) kept in sync with `ReflectSdkNodeKind` plus documented extras (`FieldKind`, `AssignOp`).

`src/Prelude.bd` is hand-maintained and only re-exports the modules above.

## Regenerating

From the **compiler** repository root (directory containing `crates/` and `corelib/`):

```bash
./corelib/packages/compiler-sdk/regen_mod_sdk_surfaces.sh
```

Equivalent manual invocation uses `cargo run -p beskid_ast_reflect_gen` with `--no-banner` and `--no-reflect-stub` when stitching into the checked-in files; see the script for exact allowlists.

After changing reflected Rust sources (`compiler_sdk_reflect.rs`, any `crates/beskid_analysis/src/syntax/**` AST struct or enum, `hir/item.rs`, or diagnostic enums), re-run the script and commit the updated `.bd` files (including `Syntax/Nodes/` and `tests/expected/syntax_nodes_inventory.txt` in `beskid_ast_reflect_gen` when the inventory changes).
