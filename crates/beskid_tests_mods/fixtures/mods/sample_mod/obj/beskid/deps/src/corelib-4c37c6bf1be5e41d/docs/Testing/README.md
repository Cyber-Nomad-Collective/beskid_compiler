**Testing** contains **`Testing.Contracts`** (protocol-style contracts for deferred assertions) and **`Testing.Assertions`** (concrete `Assert*` and **`Fail`** entry points). Both are re-exported from **`Prelude.bd`**.

## Modules

- [Testing.Contracts](./Contracts.md) — `AssertionPredicate`, `AssertionMessageBuilder`
- [Testing.Assertions](./Assertions.md) — `Fail`, `AssertTrue`, `AssertEqual*`, `AssertContains`, …

## Integration

- Assertion functions use **`System.Output.WriteLine`** and intentional integer division by zero in a private **`trigger_failure`** path to stop the test—behavior matches the checked-in `Assertions.bd`.
