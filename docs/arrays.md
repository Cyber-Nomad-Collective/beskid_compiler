# Runtime Arrays (v1.0)

## Current Behavior
- `array_new(elem_size, len)` allocates array header and (optionally) backing storage via `arrays_backing` feature.
- Capacity policy is fixed-capacity (`cap == len`) in current runtime ABI surface.

## Invariants
- Array header is runtime-allocated and non-null on success.
- Without `arrays_backing`, payload pointer is null and only header semantics are guaranteed.

## Deferred
- Builtin element-wise get/set/copy helpers are deferred to post-v1.0 ABI expansion.
- Growable capacity policy is deferred; callers should treat v1.0 arrays as fixed-capacity.
