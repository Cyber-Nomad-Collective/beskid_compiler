# beskid_hi

Reference extension crate for the pluggable `beskid hi` shell. Depends only on `beskid_tools::shell`.

## Layout

```
src/
  lib.rs          # hub: mod + pub use only
  register.rs     # register_widgets()
  models/         # ExtensionWidgetDescriptor, board fragment
  widgets/        # BeskidWidget implementations
assets/
  board.fragment.bsol
```

## Extension contract

1. Declare widgets in `models/descriptor.rs` (`WIDGET_CATALOG`).
2. Implement `BeskidWidget` under `widgets/`.
3. Register via `register_widgets()` (descriptors + widget instances).

`beskid_cli` links this crate and passes `beskid_hi::register_widgets` into `ShellHost::run_hi_blocking`.

## Board fragment

Copy `assets/board.fragment.bsol` into your scope layout (`.beskid/board.bsol` or `~/.beskid/data/boards/default.board.bsol`) as a `node` block referencing `hi.hello`.
