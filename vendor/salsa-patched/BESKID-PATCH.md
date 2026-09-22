# Salsa snapshot replay patch

## Provenance

This directory contains the published `salsa` 0.26.2 crate, mechanically extracted
from its crates.io archive. Archive SHA-256:
`4612ff789805e65c87e9b38cb749a293212a615af065bed8a2001086801498c3`.
The upstream `.cargo_vcs_info.json` records commit
`c9114d4bde31df4a17a0edf24d9356e131631b78` in
<https://github.com/salsa-rs/salsa>. Preserve the upstream MIT and Apache licenses.

The root workspace patches crates.io resolution to this package for every
consumer. It is an explicit non-default member so focused upstream regressions
use the single root lockfile. The archive's nested Cargo.lock is intentionally
omitted; all other upstream files are preserved except the delta below.

## Local delta

- `src/function.rs`: initialize the existing per-function typed downcaster from
  the owning database's registered `Views` when an erased dependency or
  accumulator traversal reaches a restored memo before direct query access.
  Missing views still fail closed. No new cast, query evaluation, cache eviction,
  or serialization representation is introduced.
- `src/ingredient.rs` and `src/function/accumulated.rs`: pass the already-available
  owning `Zalsa` through accumulator traversal to the same shared accessor.
- `tests/persistence_replay.rs` and its Cargo target: fresh custom-view database
  replay, revision change, parent-before-child dependency validation, and
  rejection of an unregistered dependency view. A separate test binary avoids
  changing upstream persistence golden ingredient indexes.

The compiler registers its own typed `Db` view on the candidate before publishing
restored storage. This is a view declaration, not a per-query warm-up inventory.
The patch does not add support for serializing accumulator values, which upstream
0.26.2 does not support.

## Verification and removal

Run from the root workspace:

```sh
cargo test -p salsa --features persistence --test persistence_replay --test persistence --test accumulate-chain
cargo test -p beskid_queries --features persistence --test persistence
```

Retain this small delta until an upstream release provides equivalent typed
initialization semantics and passes both these regressions and repeated compiler
snapshot replay. Then remove the override and vendor directory together; do not
leave competing Salsa implementations or a compatibility fallback.
