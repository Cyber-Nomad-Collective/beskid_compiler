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
| `NavRegistry` / `NavRegistrar` | Hierarchical hamburger menu entries |
| `ToolSettingsRegistry` / `ToolSettingsRegistrar` | Per-tool settings pages + BSOL persistence |
| `ShellHost` | Ratkit runner for `beskid hi` |
| `ShellChrome` | Footer hotkeys (shortcuts live here only) + header with menu affordance |
| `PagesDoc` | `shell.pages.v1` — linked dashboards / page roots in one board file |
| `HiLayoutState` | Panes runtime + board.v2 + pages + layout editor |
| `CommandItem` | Palette entry: CLI subprocess or contextual in-shell action |

### Extension registrars

```rust
ShellHost::run_hi_blocking(
    scope,
    plain,
    &[my_crate::register_widgets],
    &[my_crate::register_nav],
    &[my_crate::register_settings],
)?;
```

### Board layout (`board.v2` BSOL)

Layouts are flex trees lowered to [panes](https://docs.rs/panes/) and resolved with [panes-ratatui](https://docs.rs/panes-ratatui/). Footer chrome stays **outside** the panes tree.

| Scope | Board path | Pages path |
|-------|------------|------------|
| Workspace | `<ws>/.beskid/board.bsol` | `<ws>/.beskid/pages.bsol` |
| Project | `<proj>/.beskid/board.bsol` | `<proj>/.beskid/pages.bsol` |
| User | `~/.beskid/data/boards/default.board.bsol` | `~/.beskid/data/pages/default.pages.bsol` |

Multi-page boards embed several root subtrees (`root`, `graphs_root`, …). `shell.pages.v1` maps page ids to `board_root` node ids.

### Navigation (`m` / `☰`)

Hamburger menu merges built-in nav (`beskid > compiler > …`) with scope `pages.bsol` nav items and extension `NavRegistrar` entries. Selecting a page switches `doc.root` and rebuilds the panes runtime.

Built-in pages: Home, Graphs, Compile/Debug, Analysis, Settings, Packages, New project, Debugger (reserved).

### Layout editor

`Layout: Edit` opens a **non-blocking** right drawer (Templates / Widgets / Layouts / Structure tabs) while the dashboard stays interactive. Global keys (`Ctrl+P`, `q`, `m`, `+`/`-`, `Esc`) still work.

Templates: holy-grail, sidebar-main, single-focus, dashboard-grid. Widget list uses `WidgetRegistry::descriptors()`.

### Tool settings

`tools.config.v1` BSOL at `~/.beskid/config/tools.bsol` with optional `<root>/.beskid/tools.bsol` overrides. Built-in pages: shell, pckg, templates. Rendered by `shell.settings` widget.

### Command palette

Global shortcuts are shown only in the **footer chrome**: `Ctrl+P` / `:`, `?`, `q`, `m` (menu). No separate shortcuts panel in the default board.

See `beskid_hi` for extension widgets, nav catalog, and a copyable `board.fragment.bsol`.
