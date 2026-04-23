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

## Tests

Integration tests live under `crates/beskid_tests/src/format/`: golden output and idempotent `format → parse → format` checks.

## Limitations

- No preservation of comments or non-semantic trivia (the AST has no comment nodes).
- Top-level `EmitCtx::indent` starts at zero, so file-level items are not prefixed with an extra indent column unless nested (for example inside an inline `mod` body).
