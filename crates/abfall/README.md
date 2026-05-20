# Abfall — concurrent tri-color tracing GC

This directory is a **vendored copy** inside the [Beskid compiler](https://github.com/Cyber-Nomad-Collective/beskid_compiler) workspace (`compiler/crates/abfall`). It extends the upstream design with hooks required by the Beskid runtime and codegen. When contributing GC fixes, prefer aligning with upstream where possible; Beskid-specific behavior lives in `src/beskid.rs` and the `Heap::allocate_beskid` / `write_barrier` paths.

## Features

- **Tri-color marking** — white (candidate), gray (discovered), black (scanned)
- **Concurrent mark-and-sweep** — background collector with configurable pacing
- **Thread-safe heap** — `Arc<Heap>` shared across mutator threads (Phase B)
- **External roots** — register stack/global roots from the runtime
- **Manual control** — disable auto collection, `force_collect`, incremental assist work

## Beskid integration

| Piece | Role |
|-------|------|
| `TypeDescriptor` / `BeskidObject` | Opaque heap objects allocated via `Heap::allocate_beskid`; pointer layout comes from codegen-emitted descriptors |
| `Heap::write_barrier` | Dijkstra-style insertion barrier during marking; surfaced as `gc_write_barrier` in `beskid_runtime` |
| `GcPhase` | Exposed to builtins (`Idle` / `Marking` / `Sweeping`) for diagnostics and tests |
| `enter_heap_session` / `with_current_heap` | TLS heap session used while the engine runs a fiber mutator |
| `beskid_runtime` / `beskid_engine` | Path dependency `abfall = { path = "../abfall" }` |

Generated stores that may create white→black edges call the runtime export `gc_write_barrier(parent, child)` (see platform spec: [Memory and GC runtime contract](https://github.com/Cyber-Nomad-Collective/beskid/blob/main/site/website/src/content/docs/platform-spec/execution/runtime/memory-and-gc-runtime-contract/index.mdx)).

Functional tests: `compiler/crates/beskid_runtime/tests/gc.rs`, `gc_concurrency.rs`, and `compiler/crates/abfall/tests/gc_functional.rs`.

## Architecture

### Tri-color algorithm

1. **Mark** — roots gray; drain gray queue (trace fields / `BeskidObject` pointer slots); turn gray → black
2. **Sweep** — unlink white headers and reclaim

### Beskid object tracing

`BeskidObject` loads child pointers at offsets from `TypeDescriptor::pointer_offsets` and calls `Heap::mark_payload_ptr` during trace. Descriptors are static data emitted by `beskid_codegen` module emission.

## Usage (library)

### `GcContext` (ergonomic API)

```rust
use abfall::GcContext;

let ctx = GcContext::new();
let value = ctx.allocate(42);
assert_eq!(*value, 42);
```

### `Heap` (Beskid runtime style)

```rust
use abfall::{GcOptions, Heap};
use std::sync::Arc;

let heap = Arc::new(Heap::new(GcOptions::default()));
let payload = heap.allocate_beskid(size, type_desc_ptr);
// pointer stores during marking:
heap.write_barrier(parent_payload, child_payload);
```

### Concurrent mutators

```rust
use abfall::Heap;
use std::sync::Arc;
use std::thread;

let heap = Arc::new(Heap::new(GcOptions::default()));
// each mutator thread: enter_heap_session(&heap) before allocating
```

## License

licensed under **MIT** per upstream choice clause [HellButcher/abfall](https://github.com/HellButcher/abfall).

- [LICENSE-MIT](LICENSE)

Contributions intended for upstream should be submitted to the original repository when they are not Beskid-specific.
