# corelib_interop

Interop view types for the C ABI boundary.

Per the `language-meta--interop--interop-contracts` spec, Beskid `string` and `T[]` MUST NOT cross the user FFI boundary as ordinary GC references. They cross as interop view types:

- `CStringView` — `{ptr, len}` aligning with `BeskidStr`
- `CBuffer` — `{ptr, len}` for byte ranges
- `CArrayView` — `{ptr, len, cap}` aligning with `BeskidArray`

This package is consumed by `Beskid.Glue` and by user extern contracts that need view types.
