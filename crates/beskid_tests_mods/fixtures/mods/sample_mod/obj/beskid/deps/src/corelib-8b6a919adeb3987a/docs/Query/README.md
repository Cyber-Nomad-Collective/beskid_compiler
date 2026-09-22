**Query** modules model array-backed query pipelines: **`Query.QueryState`** holds iterator state; **`Query.Operators`** exposes combinators; **`Query.Execution`** provides deferred/materialization helpers.

Optional values use **`Core.Optional`** — not a separate query contracts module.

## Modules

- [Query.Operators](./Operators.md) — `QueryState`, `FromArray`, `Where`, `Select`, `Take`, `Skip`, `Count`, `First`, `ToList`, `Any`, `All`, `OrderBy`, `CollectArray`
- [Query.Execution](./Execution.md) — `IsDeferred`, `MaterializeCount`
