# ABI-v5 typed managed arrays

`beskid_rt_v5_array_allocate(BeskidArrayAllocationRequest*)` is the only native allocation
entrypoint for syntax-lowered managed array literals. The historical `array_new(element_size,
length)` entrypoint remains a byte-storage compatibility surface and is not sufficient authority
for pointer-bearing elements.

The request references immutable compiler-emitted `BeskidArrayElementDescriptor` data:

- `stride` and `alignment` describe one element;
- `pointer_map` contains strictly ascending, pointer-aligned offsets relative to an element base;
- `pointer_count` names exactly the map entries.

The native runtime validates the complete request before allocation. It allocates the stable
three-word `BeskidArray { ptr, len, cap }` header and all backing bytes as one GC object, copies
the validated descriptor metadata into that object's tracing state, and rejects overflow,
untrusted flags, malformed maps, and foreign publication targets. The collector scans every
element/map pair on every mark pass and removes the range ownership entry while sweeping.

Pointer writes use `beskid_rt_v5_array_write_barrier`; scalar writes do not. No path infers a
pointer map from element size and no HIR/Lowerable fallback is allowed.

## Required acceptance evidence (intentionally unexecuted in the source-first phase)

1. A scalar `i32[]` literal emits a static request, calls `beskid_rt_v5_array_allocate`, loads
   `BeskidArray.ptr`, and produces stock-verified CLIF without a stack-backed array literal.
2. A `string[]`/pointer literal emits map `[0]`, writes every element, calls the array write
   barrier, survives forced GC, and supports a later collection after the original expression has
   returned.
3. Nested arrays, interior string-slice pointers, replacement during concurrent marking, and
   removal of the final root are verified on Linux x86_64, macOS AArch64, and Windows x86_64.
4. Null request, zero stride, invalid alignment, wrapped byte count, duplicate/out-of-range map
   offsets, non-zero flags/reserved, and foreign barrier destinations fail closed.
5. Generated ABI artifacts from `runtime_manifest.bsol`, the runtime kit audit, and Cranelift
   verifier output agree on the final symbol and all three manifest layouts.
