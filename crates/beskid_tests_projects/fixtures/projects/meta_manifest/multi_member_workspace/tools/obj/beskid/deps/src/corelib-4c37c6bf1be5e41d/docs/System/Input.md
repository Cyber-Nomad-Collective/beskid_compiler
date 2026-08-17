`System.Input` exposes **stdin-only** read helpers built on **`System.Syscall.ReadWith`**.

## Functions

| Function | Behavior |
|----------|----------|
| `Read()` | Reads up to the default byte limit from stdin; returns `Result<string, SyscallError>`. |
| `ReadLine()` | Reads bytes until `\n` or EOF; returns the line without the newline. |

## Contract

- Reads are routed to **`Descriptor::Standard(StandardStream::Stdin)`** only.
- Non-stdin fds remain unsupported at the syscall layer.
