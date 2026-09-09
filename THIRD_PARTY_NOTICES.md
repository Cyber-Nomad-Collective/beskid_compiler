# Third-party notices

The compiler source tree includes the following vendored projects. They retain
their upstream licenses and are not relicensed by the compiler's Apache-2.0
license.

- `crates/abfall`: MIT OR Apache-2.0. See `crates/abfall/LICENSE`.
- `vendor/ratkit`: MIT. Copyright 2026 Alpha Innovation Labs. See
  `vendor/ratkit/LICENSE`.
- `vendor/cargo-cross-patched`: MIT. Copyright 2025 zijiren. See
  `vendor/cargo-cross-patched/LICENSE`.

The exact release dependency graph is recorded in `Cargo.lock`. Packagers must
retain all notices required by the dependencies included in their binary build.
