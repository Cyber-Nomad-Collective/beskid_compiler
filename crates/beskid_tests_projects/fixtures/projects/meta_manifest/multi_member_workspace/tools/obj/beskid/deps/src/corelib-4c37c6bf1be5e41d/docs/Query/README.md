**Query** modules model deferred query-like pipelines: **`Query.Contracts`** defines **`Option`** and the **`Iterator`** contract; **`Query.Operators`** holds **`QueryState<T>`** and combinators; **`Query.Execution`** exposes small helpers over that state.

These sources are still **staged**—several functions use placeholder logic to keep the project building while the language and runtime gain full query support. See each page for the exact behavior in `Query/*.bd`.

## Modules

- [Query.Contracts](./Contracts.md) — `Option`, `Iterator`, `HasValue`
- [Query.Operators](./Operators.md) — `QueryState`, `Where`, `Select`, `Take`, `Skip`, `Count`, `First`, `CollectArray`
- [Query.Execution](./Execution.md) — `IsDeferred`, `MaterializeCount`
