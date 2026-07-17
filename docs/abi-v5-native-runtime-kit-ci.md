# ABI-v5 native runtime-kit CI migration

The 0.4 release requires every linked AOT/JIT/corelib execution path to load
one exact installed ABI-v5 runtime kit. CI must not build or prepare
`beskid_runtime_bridge` as test setup.

`scripts/verify-native-runtime-kit-ci.sh` intentionally fails against the
current callers. It records a release blocker; it is not a fallback mechanism.

## Required replacement for each caller

Before `beskid_cli`, AOT, JIT, or corelib execution, the job must:

1. Build the canonical Beskid runtime through expanded syntax, Salsa facts,
   generated ISLE, and verified stock CLIF.
2. Produce both static and shared artifacts for that job's target and profile.
3. Publish those artifacts with `beskid runtime-kit build` into an otherwise
   empty prefix at `lib/beskid-runtime/abi-5/<target>/<debug|release>/`.
4. Validate `abi.json`, canonical runtime-source hash, artifact hashes, and
   the manifest import/export allowlists.
5. Export `BESKID_RUNTIME_PREFIX` to that prefix and run the existing tests
   without a source-tree archive, Rust bridge, host fallback, or profile
   fallback.

The migration is complete only when the following callers no longer invoke
`scripts/ensure-runtime-bridge.sh` and the verifier passes:

- `scripts/ci/compiler-rust-gate.sh`
- `scripts/ci/corelib-gate.sh`
- `scripts/ci/corelib-publish.sh`
- `.github/workflows/compiler-gate-testbox.yml`

The release matrix then has to run the same replacement for debug and release
on `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, and
`x86_64-pc-windows-msvc` before bridge removal is safe.
