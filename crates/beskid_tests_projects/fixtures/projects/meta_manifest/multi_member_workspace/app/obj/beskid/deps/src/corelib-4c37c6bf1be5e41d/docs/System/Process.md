`System.Process` exposes **`ProcessError`** and process-level operations backed by runtime `process_*` builtins.

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
| `Id() -> i32` | Returns **`__process_getpid()`**. |
| `Exit(i32 code)` | Terminates via **`__process_exit(code)`** (all exit codes). |
| `ExitCode() -> Result<i64, ProcessError>` | **`Ok(0)`** placeholder until child-process APIs land. |
| `Run(string command) -> Result<bool, ProcessError>` | Empty command → **`InvalidCommand`**. Otherwise **`SpawnFailed`** (spawn deferred). |
