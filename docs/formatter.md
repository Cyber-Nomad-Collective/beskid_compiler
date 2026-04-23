# Opinionated formatter (Emit)

The compiler exposes a **pretty-printer** over the concrete syntax AST: it walks parsed nodes and writes canonical text. The design mirrors the `Emit` trait and `EmitCtx` pattern from the **bsharp** reference (`references/bsharp/src/bsharp_syntax/src/emitters/`): `Emit::emit` writes into a `fmt::Write` while `EmitCtx` tracks indentation and spacing policy.

## API

- Crate: `beskid_analysis`
- Module: `beskid_analysis::format`
- Entry point: `format::format_program(&Spanned<Program>) -> Result<String, EmitError>`
- Lower-level: `format::Emitter::write`, `format::Emit` for extending emission

Parsing must succeed first (for example via `beskid_analysis::services::parse_program`).

## Layout rules (C#-style)

- **Indentation**: 4 spaces per nesting level (`EmitCtx::push_indent` / `pop_indent`).
- **Braces**: Allman-style braced bodies: newline before `{`, indented contents, closing `}` aligned with the opening line’s outer indent (see `EmitCtx::open_brace` / `close_brace` and `Block` emission in `format/emit.rs`).
- **Control flow headers**: `if` and `while` emit a parenthesized condition (`if (expr)`, `while (expr)`) while preserving Beskid’s grammar (any expression may appear inside the parentheses).
- **Blank lines** (`format/policy.rs`):
  - One separating newline between consecutive top-level items (`between_top_level_declarations`).
  - One separating newline between members inside `type`, `enum`, `contract`, and `attribute` bodies (`between_members`).
  - Inside statement blocks, an extra newline when a control-flow statement (`if`, `while`, `for`) is immediately followed by a `let` (`between_block_items`), matching the spirit of CSharpier-style spacing.

Empty braced bodies are emitted as `{ }` (space between braces) where applicable.

## Coverage matrix

Current formatter coverage in `beskid_analysis::format`:

| Syntax family | Status | Notes |
|---|---|---|
| Program / top-level nodes | Implemented | Includes `leading_docs` emission for `Program` and nested `InlineModule`. |
| Items (`use`, `mod`, inline `mod`, `type`, `enum`, `contract`, function, method, attributes) | Implemented | Item emitters are split into `format/items/*` modules. |
| Structural `ImplBlock` | Implemented | `Emit` exists for `syntax::items::impl_block::ImplBlock`; parser still flattens `impl` blocks into methods in `Program`/`InlineModule`. |
| Statements | Implemented | Includes `if`/`while` parentheses and spacing policy hooks. |
| `RangeExpression` | Implemented | Emits canonical `range(start, end)`. |
| Expressions | Implemented | Full `Expression` enum surface in `expressions_emit.rs`. |
| Types / paths / params / fields | Implemented | In `types_emit.rs`. |

Formatting of comments/trivia outside `leading_docs` remains out of scope.

## Tests

Integration tests live under [`crates/beskid_tests/src/format/`](../crates/beskid_tests/src/format/) and [`crates/beskid_tests/fixtures/format/`](../crates/beskid_tests/fixtures/format/) (including **recursive** subdirectories such as `expr/`, `stmt/`, `items/`, `edges/`).

- Golden output tests (`*.input.bd` / `*.expected.bd` pairs).
- Idempotence checks (`format(parse(format(parse(x))))` stability).
- Parse-preservation: top-level item kinds (with `method` / `function` relaxed for `impl` round-trip) plus **statement-count parity** across the program.
- Structural emitter checks for `ImplBlock` (Pest fragment, not full `Program` path).
- **CLI smoke in CI**: after `format_regression`, GitHub Actions runs `beskid format --check` on every `*.expected.bd` golden under `fixtures/format/` (canonical files must already match formatter output).

See **[formatter-test-matrix.md](formatter-test-matrix.md)** for the mapping of syntax areas to fixtures and `#[test]` names.

**Regenerate expected files** after intentional formatter changes:

```bash
cargo build -p beskid_cli
python3 scripts/bless_format_fixtures.py
```

**Optional corelib gate** (opt-in): `BESKID_FORMAT_CORPUS=1 python -m nox -s format_corpus_corelib` runs `scripts/format_corpus_check.py`.

## Limitations

- No preservation of comments or non-semantic trivia (the AST has no comment nodes).
- Top-level `EmitCtx::indent` starts at zero, so file-level items are not prefixed with an extra indent column unless nested (for example inside an inline `mod` body).
- The LSP range-formatting endpoint currently applies **full-document** replacement strategy (not partial AST/range formatting).
- On parse failure, formatter/LSP formatting handlers return no edits instead of best-effort rewrite.
