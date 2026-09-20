# Core.Collections.Stack

`Stack<T>` owns `T[] storage` and `i64 count`.

`Push`, `Pop`, `Peek`, `Count`, and `IsEmpty` are owning receiver methods and preserve LIFO order. `Peek` returns an error for an empty stack. `New<T>` is the module constructor.
