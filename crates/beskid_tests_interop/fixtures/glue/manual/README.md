# Manual Rust C ABI basis

This standalone fixture prepares the full 17-representation manual import/export surface. It is separate from production runtime ABI-v5 and from the serialized stdio protocol. `build.json` records the frozen profile, ownership rules, statuses, limits and outstanding obligations.

Run from the release checkout:

```
python3 compiler/crates/beskid_tests_interop/fixtures/glue/manual/build.py \
  --output .build/glue-manual/native \
  --candidate .build/compiler/debug/beskid_cli
```

The runner invokes direct argv with bounded subprocess duration, retains stdout/stderr and their hashes, records exact tool/source/artifact hashes, builds a native Rust shared library and links/runs an ordinary C caller against it. MSVC runs require `cl` in the native developer environment. No cross compilation substitutes native execution. The Rust self-check binary verifies all fixed-width boundary values, IEEE bits including negative zero and NaN payloads, strict bool/Unicode, NUL and invalid UTF-8, bounded views, unit, checked native width, owner-release and stale-token rejection, and panic/error status conversion. Its panic is caught inside the Rust wrapper; panic-abort and signals remain process failures.

Views require caller-valid readable storage and cannot outlive their owner. Null is allowed only for zero length; the size bound is checked before dereferencing. Invalid arbitrary addresses cannot be proved safe by a length check. The fixture registry creates monotonic library-local tokens, validates declared session/shape, and never reuses a released token. It does not demonstrate a production multi-library/session authority. Owned view access and release must be serialized by the owner. No pointer is serialized.

`PrimitivePrerequisite.bd` is an ordinary Beskid semantic reproduction for missing i8/i16/u16/u64/f32. `ManualImport.bd` declares exact local C signatures and typed call adapters for all rows. `ManualExport.bd` declares the corresponding actual Beskid source shapes: its typed bool/char/string/array/unit declarations require canonical ABI adapter lowering rather than pretending that managed source values already have the C wrapper layout. Their current failures are required dependency evidence, never unsupported rows marked successful. They are sources for the eventual consumer/export executable fixtures; runtime-entry/root management and forced collection cannot be proven by the Rust/C caller.

The receipt explicitly sets `beskid_glue_qualified:false`. A passing runner means Rust/C preproof only. Beskid executable import/export, canonical buffer/handle adapters, checked scalar coercion, GC ownership, generated equivalence, shared serialization/stdIO lifecycle, installed consumption and all three target executions remain required.

## Actual manual gates

`src/ManualImportTests.bd` is the authoritative executable import fixture: 17 representation cases and three invalid/lifecycle/failure cases. `manual_tests.bproj` supplies real extern library linkage; `native_gate.py` stages its sources into a disposable project with an explicit source-bound shim search path and private home/config. `export_caller.rs` links only the shared library built from `src/ManualExport.bd`, and `export_bootstrap.c` uses the generated canonical runtime ABI header for state size/alignment and initialization. It does not substitute the Rust shim for Beskid exports.

Run only with the serialized candidate/native gate owner:

```sh
python3 native_gate.py --candidate /absolute/candidate/beskid_cli --runtime-prefix /absolute/matching/native-kit --corelib-root /absolute/checkout/compiler/corelib --output /absolute/disposable/glue-manual-v1
```

The retained receipt must show all 20 exact import case names passing with zero failures, skips, or timeouts, and a successful Rust caller built against the actual Beskid library. `export_boundary.h` freezes the normalized managed ABI: signed status, checked out parameter, and owned token. A missing managed adapter, owned release symbol, canonical token authority, or logical checked opaque-handle contract is a real prerequisite failure. The current `u64` export identity proves width only, not checked opaque ownership. The existing foreign Rust fixture registry is confined to foreign shim tests and does not authorize Beskid-owned transfers. Invalid scalar values require a checked failure wrapper; direct scalar exports alone do not cover that obligation. This fixture receipt cannot qualify generated parity, stdio/process obligations, or a three-target release.
