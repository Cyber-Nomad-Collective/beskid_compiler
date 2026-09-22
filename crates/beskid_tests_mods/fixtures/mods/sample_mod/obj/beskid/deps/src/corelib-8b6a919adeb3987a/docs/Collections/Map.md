# Core.Collections.Map

`Map<TKey, TValue>` owns `MapEntry<TKey, TValue>[] entries` and `i64 count`. It uses linear key lookup.

`Insert`, `Get`, `ContainsKey`, `Remove`, `Count`, and `IsEmpty` are owning receiver methods. Inserting an existing key replaces its value without changing count. Removing a key preserves all remaining entries. `New<TKey, TValue>` is the module constructor.
