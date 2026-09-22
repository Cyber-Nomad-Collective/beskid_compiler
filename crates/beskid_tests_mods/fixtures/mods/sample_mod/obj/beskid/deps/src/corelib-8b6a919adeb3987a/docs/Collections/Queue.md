# Core.Collections.Queue

`Queue<T>` owns `T[] storage`, `i64 head`, and `i64 count`.

`Enqueue`, `Dequeue`, `Peek`, `Count`, and `IsEmpty` are owning receiver methods and preserve FIFO order. `Peek` returns an error for an empty queue. `New<T>` is the module constructor.
