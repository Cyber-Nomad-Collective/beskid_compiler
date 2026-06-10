# beskid_hi

Reference extension crate for the pluggable `beskid hi` shell. Depends only on `beskid_tools::shell`.

## Layout

```
src/
  lib.rs          # hub: mod + pub use only
  register.rs     # register_widgets(), register_nav()
  models/         # WIDGET_CATALOG, NAV_CATALOG, board fragment
  widgets/        # BeskidWidget implementations
assets/
  board.fragment.bsol
```

## Extension contract

1. Declare widgets in `models/descriptor.rs` (`WIDGET_CATALOG`).
2. Optional nav entries in `models/nav.rs` (`NAV_CATALOG`).
3. Implement `BeskidWidget` under `widgets/`.
4. Register via `register_widgets()` and `register_nav()`.

`beskid_cli` wires all registrars into `ShellHost::run_hi_blocking`.

## Board fragment

Copy `assets/board.fragment.bsol` into your scope layout as a `node` block referencing `hi.hello`.
