# Deterministic stdin for JIT tests

## Finding

The runtime must keep its normal blocking `read(2)` behavior. POSIX specifies that a blocking read
from an empty pipe waits while any writer remains open, but returns zero (EOF) when no writer is
open. macOS likewise documents zero as the EOF result and `EAGAIN` only for descriptors explicitly
configured for non-blocking I/O.

The matrix supervisor currently spawns its isolated test worker without configuring stdin. The
worker therefore inherits the supervisor's descriptor 0, which may be an open terminal or an open
automation pipe with no bytes. JIT code correctly blocks in the ABI-v5 syscall worker when an input
test calls `Core.Input.Read()`.

The deterministic test boundary is the isolated matrix worker process: connect its stdin to the
null device so reads immediately observe EOF. Rust's process API supports this directly with
`Command::stdin(Stdio::null())`. This changes only automated test-process setup and leaves runtime
and ordinary application input semantics intact.

## Primary sources

- [POSIX `read()`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/read.html)
- [Apple `read(2)` manual page](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/read.2.html)
- [Rust `std::process::Command`](https://doc.rust-lang.org/std/process/struct.Command.html)
