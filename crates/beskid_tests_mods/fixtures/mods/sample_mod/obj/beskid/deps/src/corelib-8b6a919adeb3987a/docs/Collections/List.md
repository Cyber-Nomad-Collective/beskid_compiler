# Core.Collections.List

`List<T>` owns `T[] storage` and `i64 count`. `Push`, `Pop`, `Get`, `Count`, and `IsEmpty` are owning receiver methods; `New<T>` is the module constructor.

Growth preserves every retained value in order. `Get` returns `Result::Error("index out of range")` when `index < 0` or `index >= Count()`.
