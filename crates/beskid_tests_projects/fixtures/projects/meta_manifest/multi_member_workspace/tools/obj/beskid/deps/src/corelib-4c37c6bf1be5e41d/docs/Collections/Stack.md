`Collections.Stack` mirrors **`Collections.Queue`**: only the type shell and **`Count`** exist until push/pop are added.

## Type

```beskid
pub type Stack<T> {
    i64 count,
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `New<T>() -> Stack<T>` | Stack with `count = 0`. |
| `Count<T>(Stack<T> stack) -> i64` | Returns `stack.count`. |
