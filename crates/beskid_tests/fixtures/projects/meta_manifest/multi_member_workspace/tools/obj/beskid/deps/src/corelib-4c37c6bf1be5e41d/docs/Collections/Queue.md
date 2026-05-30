`Collections.Queue` currently exposes only container shape and **`Count`**—enqueue/dequeue operations are not yet in the sources.

## Type

```beskid
pub type Queue<T> {
    i64 count,
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `New<T>() -> Queue<T>` | Queue with `count = 0`. |
| `Count<T>(Queue<T> queue) -> i64` | Returns `queue.count`. |
