# beskid_ast_reflect_gen

Semi-automatic **Beskid** (`.bd`) generation from selected **Rust** sources in `beskid_analysis`:

- **v1 stubs** (`generate_from_paths`): `enum` / `type` shapes with `ReflectStub` payloads (legacy single-file stitching).
- **Syntax node pack** (`syntax_nodes::emit_syntax_sdk`): one `.bd` per public syntax AST type under `compiler-sdk/src/Beskid/Syntax/Nodes/`, plus concrete list/optional helpers (`IdentifierList`, `OptionalType`, …). `Vec<T>` / `Option<T>` map to those helpers when `T` peels to a known syntax node (including `Spanned<T>` → `T` for naming). `Syntax.bd` stays thin and does **not** declare `ReflectStub`. Inventory is aligned with `ReflectSdkNodeKind` (see `tests/expected/syntax_nodes_inventory.txt`). Stale `Syntax/Nodes/*.bd` from older runs are removed on regen.
  - **Field names:** struct fields use Beskid **lowerCamel** derived from Rust `snake_case` (tuple structs: `field_N`); enum tuple variants use `payload` / `variant_field_N`; Beskid reserved words and names whose first snake segment is a keyword (`contract_name`, …) get a leading `_` so they remain valid identifiers (see `src/emit_idents.rs`).
  - **Docs:** type headers use plain `///` text plus **`@variant` / `@par`** where applicable. Per-field lines come from mirrored Rust `///` attributes only (no synthetic Beskid/Rust boilerplate); `ReflectStub` fields append a short note. The type-level struct index lists field names once; `@arg` is reserved for callable parameters in hand-authored code. We do not emit `@ref` in generated SDK sources.
  - **v1 stitched stubs** (`generate_from_paths` / `emit_enum_variant` in `lib.rs`): tuple payloads use the same `payload` / `variant_field_N` naming; named enum fields use Rust names with the same Beskid identifier escaping.

## Invocation

From the **compiler** workspace root (`compiler/Cargo.toml`):

```bash
# Emit Mod SDK syntax node tree (thin Syntax.bd + Syntax/Nodes/*.bd); requires compiler workspace root
cargo run -p beskid_ast_reflect_gen -- --workspace . --emit-syntax-sdk ./corelib/packages/compiler-sdk/src/Beskid/Compiler

# Built-in allowlist (syntax `node.rs`) — writes to stdout
cargo run -p beskid_ast_reflect_gen -- --workspace .

# Explicit inputs
cargo run -p beskid_ast_reflect_gen -- --out /tmp/reflect.bd crates/beskid_analysis/src/syntax/items/node.rs

# Only items tagged with #[beskid_reflect] (for curated extraction)
cargo run -p beskid_ast_reflect_gen -- --only-annotated -- tests/fixtures/mirror_reflect.rs

# Curated public items by Rust name (comma-separated)
cargo run -p beskid_ast_reflect_gen -- --items Node -- crates/beskid_analysis/src/syntax/items/node.rs

# Stitching helpers: omit default banner and ReflectStub prelude
cargo run -p beskid_ast_reflect_gen -- --no-banner --no-reflect-stub -- ...
```

Optional: if `OUT_DIR` is set and `--out` is omitted, the tool writes `$(OUT_DIR)/ast_reflect/generated.bd` (useful from build scripts).

Checked-in Mod SDK `.bd` regeneration for corelib is scripted at `corelib/packages/compiler-sdk/regen_mod_sdk_surfaces.sh` (run from the compiler workspace root).

## Annotation

Place `#[beskid_reflect]` (or `#[beskid_ast_derive::beskid_reflect]` in `beskid_analysis`) on a `pub enum` or `pub struct` you want emitted when using `--only-annotated`. The attribute is interpreted by this tool’s `syn` pass; in analysis sources use the real no-op attribute macro from `beskid_ast_derive`.

## Tests

```bash
cargo test -p beskid_ast_reflect_gen
```

Golden output is checked with `include_str!` against `tests/expected/mirror_reflect.generated.bd` and `tests/expected/syntax_nodes_inventory.txt` (sorted syntax surface type names).
