# Formatter (implementation overview)

This document mirrors the platform-spec **[formatter feature hub](../../../../site/website/src/content/docs/platform-spec/tooling/formatter/index.mdx)** from the implementation side. Changes that touch layout, indent rules, blank-line policy, or public surface **must** update both files in the same change set; the spec is canonical for the contract, this document is canonical for the code map.

## What lives where

| Concern | Module / file |
| --- | --- |
| Public surface | `compiler/crates/beskid_analysis/src/format/mod.rs` (`pub use emit::{Emit, EmitCtx, EmitError, Emitter, emit_error_semantic_diagnostic, format_program}`). |
| `Emit` trait, `EmitCtx`, `Emitter`, `EmitError`, `format_program` | `compiler/crates/beskid_analysis/src/format/emit.rs` |
| Blank-line / between-member / between-block-item policy | `compiler/crates/beskid_analysis/src/format/policy.rs` |
| Per-construct `Emit` impls | `compiler/crates/beskid_analysis/src/format/items/`, `expressions_emit.rs`, `statements_emit.rs`, `types_emit.rs` |
| CLI entry point | `compiler/crates/beskid_cli/src/commands/format.rs` (`beskid format` / alias `beskid fmt`) |

`mod.rs` is exports-only; do not add business logic or large `impl` blocks there.

## Public surface

```rust
pub use emit::{Emit, EmitCtx, EmitError, Emitter, emit_error_semantic_diagnostic, format_program};
```

- **`format_program(&Spanned<Program>) -> Result<String, EmitError>`** is the supported programmatic entry point.
- **`emit_error_semantic_diagnostic(...)`** renders a Miette-compatible diagnostic for layout failures.
- Per-construct `Emit` implementations and the lower-level helpers (`EmitCtx::write_indent`, `EmitCtx::open_brace`, …) are part of the trait contract but are **internal**: do not depend on them from outside `beskid_analysis::format`.

## Layout policy (normative; mirrors platform-spec hub)

1. **Indent unit:** four spaces. No tabs.
2. **Blank lines:**
   - Between top-level declarations: exactly one blank line (`between_top_level_declarations`).
   - Between members of a container when `policy_blank_line_between_members` is true: exactly one blank line (`between_members`).
   - Inside a block, between a block-like statement (`if`/`while`/`for`) and a following `let`: exactly one blank line (`between_block_items`); otherwise zero.
3. **Braces:** open-brace stays on the introducing line; close-brace on its own line aligned with the construct (`EmitCtx::open_brace` / `close_brace`). Empty block emits as `{ }`.
4. **Trailing whitespace:** none. Every line ends without horizontal whitespace.
5. **Final newline:** every formatted file ends with exactly one `\n`.
6. **Attributes:** one attribute per line above the construct it decorates.
7. **Generics:** type names with generics (`Option<T>`, `Channel<T>`) emit without inner spaces and without HTML escaping.

A deviation in formatter output from this list is a defect against `beskid_analysis::format`; do not work around it in a downstream tool.

## CLI behavior cheat sheet

| Invocation | Effect |
| --- | --- |
| `beskid format <file.bd>` | Format `<file.bd>` to stdout. |
| `beskid format -o <out> <file.bd>` | Format `<file.bd>` to `<out>`. Requires exactly one input. |
| `beskid format --write <path>` | Rewrite each `.bd` under `<path>` in place. Directory inputs require `--write` or `--check`. |
| `beskid format --check <path>` | Exit non-zero on any drift with `not formatted: <path> (run \`beskid format --write <path>\`)`. CI gate. |
| `beskid fmt …` | Alias of `beskid format` (declared via clap `visible_alias = "fmt"`). |

The CLI ignores the conventional skip set when walking directories: `.git`, `.svn`, `.hg`, `target`, `node_modules`, `dist`, `.venv`, `vendor`, `__pycache__`.

## Error handling

- **Parse errors** are propagated from `services::parse_program_with_source_name`; the CLI exits non-zero **without** writing output.
- **Layout errors** are wrapped as `EmitError` and rendered via `emit_error_semantic_diagnostic`. The formatter never panics on a layout error.
- **I/O errors** (read or write) surface as anyhow contexts (`stat <path>`, `read <path>`, `write <path>`) so end users get an actionable message.

## Maintenance checklist

When you change `format/policy.rs` or `format/emit.rs::EmitCtx`:

1. Update the layout policy section above and the matching section in the platform-spec hub.
2. Regenerate any AST-side fixtures and re-run `cargo test -p beskid_analysis format::`.
3. Run `beskid format --check` against the workspace source tree (corelib, book samples) to catch unexpected drift; rerun with `--write` and review the diff before merging.
4. Add or update an ADR under `site/website/src/content/docs/platform-spec/tooling/formatter/adr/` when the change is normative (any user-visible layout change qualifies).

## Future: external formatter rule packs

External diagnostic / lint packs and possible future formatter rule packs are expected to ship as **[`packageKind: tool`](../../../../site/website/src/content/docs/platform-spec/tooling/registry-client/package-kinds/index.mdx)** registry artifacts. The current canonical formatter has no extension points — adding hooks **must** start with an ADR.
