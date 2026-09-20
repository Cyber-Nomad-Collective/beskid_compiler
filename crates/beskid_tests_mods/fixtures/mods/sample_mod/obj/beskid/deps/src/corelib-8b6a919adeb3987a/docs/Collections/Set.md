# Core.Collections.Set

`Set<T>` owns `T[] storage` and `i64 count` and uses linear equality scans.

`Add`, `Remove`, `Contains`, `Count`, and `IsEmpty` are owning receiver methods. Duplicate additions do not change storage semantics or count, and removal preserves every other retained value. `New<T>` is the module constructor.
