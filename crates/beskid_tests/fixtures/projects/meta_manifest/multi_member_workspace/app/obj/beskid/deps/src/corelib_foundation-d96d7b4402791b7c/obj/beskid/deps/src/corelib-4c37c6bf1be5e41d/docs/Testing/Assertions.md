`Testing.Assertions` provides **`Fail`** and typed **`Assert*`** helpers used from **`test`** blocks and harness-driven runs.

## Functions

| Function | Behavior |
|----------|----------|
| `Fail(string message)` | Calls **`trigger_failure`**: prints the message via **`System.Output.WriteLine`**, then forces failure via division by zero after printing a dummy value (implementation detail; guarantees abnormal termination). |
| `AssertTrue` / `AssertFalse` | Delegate to **`Fail`** when the condition is wrong. |
| `AssertEqualI64` / `AssertEqualString` | Compare expected vs actual; **`Fail`** with interpolated diagnostics on mismatch. |
| `AssertNotEqualI64` | **`Fail`** when both sides equal. |
| `AssertContains(text, needle, message)` | Uses **`Core.String.Contains`**; **`Fail`** when no match. |

## Failure shape

All failures funnel through **`Fail`**, which keeps messages consistent. Test runners classify outcomes based on process/runtime failure signaling—not shown in corelib sources directly.

## Compatibility

Names follow **`PascalCase`** and mirror common assertion library ergonomics; **`AssertContains`** depends on **`Core.String.Contains`** semantics—see [Core.String](../Core/String.md).
