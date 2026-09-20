`Core.Process` exposes current-process identity and termination through the
canonical ABI-v5 process services. Beskid 0.4 does not expose child-process
execution until one cross-platform manifest and runtime implementation exists.

## Functions

| Function | Behavior |
|----------|----------|
| `Id() -> i32` | Returns **`__process_getpid()`**. |
| `Exit(i32 code)` | Terminates via **`__process_exit(code)`** (all exit codes). |
| `CurrentId() -> i32` | Returns the current process identifier through `Id()`. |
| `IsCurrentProcess(i32 pid) -> bool` | Compares `pid` with the current process identifier. |
