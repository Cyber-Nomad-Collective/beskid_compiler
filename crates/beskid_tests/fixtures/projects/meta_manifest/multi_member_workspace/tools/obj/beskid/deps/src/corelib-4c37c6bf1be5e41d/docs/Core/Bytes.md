`Core.Bytes` provides allocation-explicit operations on `u8[]` buffers backed by runtime array builtins.

## API

| Function | Behavior |
|----------|----------|
| `New(len)` | Allocates `u8[]` via `__array_new(1, len)` |
| `Len` / `IsEmpty` | Length from `__array_len` |
| `Get` / `Set` | Indexed byte access (traps OOB) |
| `Copy` | `__bytes_copy` between buffers |
| `Compare` | `__bytes_compare` lexicographic |
| `Fill` | Byte fill loop |
| `SubSlice` | Allocating sub-range copy |
| `FromString` | `__bytes_from_str` UTF-8 octets |

## Policy

- No direct syscalls; see `System.Syscall` for fd I/O.
- String conversion for text semantics uses `Core.Encoding.Utf8`.
