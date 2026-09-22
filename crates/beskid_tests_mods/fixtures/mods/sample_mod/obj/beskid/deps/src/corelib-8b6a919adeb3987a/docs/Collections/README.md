# Core.Collections

`Core.Collections` is the only public namespace for the array-backed collection family. The former `Collections.*` path is not declared or aliased.

All receiver operations are defined on their owning public types. Modules retain only constructors and genuine array namespace helpers.

## Modules

- [Array](./Array.md) — typed arrays, direct bounds semantics, rooted growth, and iteration.
- [List](./List.md) — ordered growable values.
- [Map](./Map.md) — key/value entries with linear lookup.
- [Set](./Set.md) — unique values with linear membership.
- [Queue](./Queue.md) — FIFO values.
- [Stack](./Stack.md) — LIFO values.
