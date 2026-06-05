`System.Process` exposes **`ProcessError`** and small process-level operations.

## ProcessError

```beskid
pub enum ProcessError {
    InvalidCommand(string command),
    SpawnFailed(string command),
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `Id() -> i32` | Returns **`0`**. |
| `Exit(i32 code)` | **`code == 0`** returns normally; non-zero **`__panic_str`** (“not yet runtime-backed”). |
| `ExitCode() -> Result<i64, ProcessError>` | **`Ok(0)`**. |
| `Run(string command) -> Result<bool, ProcessError>` | Empty command → **`InvalidCommand`**. Otherwise **`SpawnFailed`** (no spawn yet). |
