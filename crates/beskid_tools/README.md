# beskid_tools

Shared command infrastructure for the `beskid` CLI: pipeline progress UI, diagnostic rendering, registry helpers, and the **pluggable shell** (`beskid hi`).

Domain crates (`beskid_repl`, `beskid_template`, `beskid_lsp`, …) own feature behavior; this crate owns cross-cutting plumbing. `beskid_cli` parses Clap args and delegates here.

## Shell extension (`beskid_tools::shell`)

The stable surface for dashboard widgets and shell chrome is **`beskid_tools::shell`**, re-exported from the crate root. The legacy `beskid_tools::tui` module remains for the compile pipeline TUI; extenders should depend on `shell` only.

### Core types

| Type | Role |
|------|------|
| `ShellScope` | `User`, `Project`, or `Workspace` — resolved from cwd or an explicit path |
| `BeskidWidget` | Pluggable tile: metadata, hotkeys, input, render |
| `WidgetRegistry` | Register built-in and extension widgets by string id |
| `ShellHost` | Ratkit runner for `beskid hi` |
| `ShellChrome` | Permanent header/footer (palette shortcut always visible) |
| `WidgetContext` | Scope, layout doc, shared `ShellState`, palette handle |
| `HiLayoutState` | Panes `LayoutRuntime` + `board.v2` doc, editor, autosave |
| `CommandItem` | Palette entry: CLI subprocess or contextual in-shell action |

### Widget contract

```rust
pub trait BeskidWidget: Send {
    fn meta(&self) -> WidgetMeta;
    fn hotkeys(&self, ctx: &WidgetContext<'_>) -> Vec<Hotkey>;
    fn contextual_commands(&self, ctx: &WidgetContext<'_>) -> Vec<ContextualCommand> { ... }
    fn on_input(&mut self, event: &ShellInput, ctx: &mut WidgetContext<'_>) -> ShellAction;
    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>);
}
```

Register widgets before launching the host:

```rust
pub type WidgetRegistrar = fn(&mut WidgetRegistry);

fn register(registry: &mut WidgetRegistry) {
    registry.register(Box::new(MyWidget));
}

ShellHost::run_hi_blocking(scope, plain, &[register])?;
```

`beskid hi` links `beskid_hi` at compile time and passes its registrar; dynamic `.so` loading is out of scope for v1.

### Board layout (`board.v2` BSOL)

Layouts are flex trees lowered to [panes](https://docs.rs/panes/) and resolved with [panes-ratatui](https://docs.rs/panes-ratatui/). The footer chrome row stays outside the panes tree.

| Scope | Path | Fallback |
|-------|------|----------|
| Workspace | `<ws>/.beskid/board.bsol` | embedded v2 default |
| Project | `<proj>/.beskid/board.bsol` | embedded v2 default |
| User | `~/.beskid/data/boards/default.board.bsol` | embedded `hi-default.board.v2.bsol` |

On load, `board.v2` is parsed directly. Legacy `board.v1` files are imported once to a v2 tree. Saves always emit `board.v2`.

Example node kinds: `col`, `row`, `split`, `tabs`, `stack`, `panel` (leaf with `widget = "hi.welcome"`).

### Layout editor

Palette command `Layout: Edit` toggles edit mode. While active:

- `+` / `-` resize the focused panel boundary
- `Esc` exits (prompts if dirty)
- Palette sub-commands: focus next/prev, add/remove panel, wrap column/row, convert to tabs/stack, set widget, save, reset

Edits auto-save to the scope path after ~500ms debounce. `Layout: Save` and `Layout: Reset` are explicit palette actions.

### Scope picker

`Open workspace` / `Open project` open a ratatui-explorer overlay filtered to `.bws` / `.bproj`. On confirm, scope reloads and the layout hot-swaps from the new scope's `board.bsol`.

Workspace wins over project when both are discovered walking parents from cwd.

### Command palette

- **CLI commands** — run `beskid <subcommand>` in a subprocess (suspend/resume terminal).
- **Contextual commands** — open overlays, focus widgets, or in-shell actions registered per scope.

Global shortcuts (footer chrome): `Ctrl+P` / `:`, `?` (help), `q` (quit). The same palette is available from the compile pipeline TUI footer.

### Built-in widget ids

| Id | Purpose |
|----|---------|
| `pipeline.compile` | Compile dashboard (build/analyze/test pipeline) |
| `tests.runner` | Test overlay |
| `pckg.browser` | Package registry browser |
| `analysis.diagnostics` | Analyze / diagnostics |
| `shell.scope` | Scope summary header |
| `hi.welcome` | Hi dashboard welcome tile |
| `shell.shortcuts` | Shortcut reference |
| `shell.log` | Log panel |
| `shell.chrome` | Footer status |

Extension crates export `WIDGET_CATALOG` + `register_widgets`; see `beskid_hi` for `hi.hello` and a copyable `board.fragment.bsol` (v2 node snippet).
