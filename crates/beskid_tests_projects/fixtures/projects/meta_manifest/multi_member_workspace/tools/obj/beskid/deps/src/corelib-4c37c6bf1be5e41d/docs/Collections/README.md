The **Collections** family lives under `src/Collections/*.bd`. These modules are **not** re-exported from `Prelude.bd` today; import the module you need explicitly.

Many APIs are still **staged**: types and function signatures are present for compilation and documentation, while backing storage or builtins may be incomplete. Each module page calls out what is implemented versus placeholder behavior.

## Modules

- [Array](./Array.md) — `ArrayIter`, iteration helpers (`Len` via **`__array_len`**).
- [List](./List.md) — `List<T>` with placeholder storage (`Get` surfaces errors).
- [Map](./Map.md), [Set](./Set.md) — keyed/unordered shapes with logical counts.
- [Queue](./Queue.md), [Stack](./Stack.md) — FIFO/LIFO shapes with counts only.
