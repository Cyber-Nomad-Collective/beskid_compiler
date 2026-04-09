# Runtime Memory Model (v1.0)

## Scope
- Runtime execution is single-threaded per `Engine::with_arena` invocation.
- GC state is bound to thread-local runtime scope pointers (`Mutation` + `RuntimeRoot`).
- Nested runtime scopes are rejected with `runtime reentrancy violation` panic.

## Invariants
- Any builtin requiring allocation or root access must execute under an active runtime scope.
- `alloc` returns zeroed memory and increments runtime allocation + heap counters.
- String and array headers are runtime-managed allocations.
- Write barriers are currently a structural hook (`gc_write_barrier`) and must be called at mutation boundaries where required by lowering.

## Reentrancy Policy
- Reentrancy is **not supported** in v1.0.
- Entering `Engine::with_arena` while already in an active runtime scope panics immediately.
- This avoids hidden nested mutation/root aliasing until explicit nested-scope semantics are designed.

## Fragmentation Notes
- v1.0 uses non-compacting allocation paths.
- Exposed counters:
  - `heap_total_bytes`
  - `heap_live_bytes`
  - `heap_fragmentation_bytes` (derived)
- Young-generation compaction remains deferred post-v1.0.
