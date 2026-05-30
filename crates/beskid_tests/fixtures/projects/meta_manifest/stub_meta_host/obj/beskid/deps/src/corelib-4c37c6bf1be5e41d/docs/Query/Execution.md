`Query.Execution` depends on **`Query.Operators`**.

## Functions

| Function | Behavior |
|----------|----------|
| `IsDeferred<T>(QueryState<T> state) -> bool` | Returns `state.count > -1` (always **`true`** for any normal count). |
| `MaterializeCount<T>(QueryState<T> state) -> i64` | Delegates to **`Query.Operators.Count`**. |

Use these as thin adapters when wiring future materialization; they do not perform I/O or allocation today.
