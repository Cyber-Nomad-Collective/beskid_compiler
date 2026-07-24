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

- No direct syscalls; see `Core.Syscall` for fd I/O.
- String conversion for text semantics uses `Core.Encoding.Utf8`.

## Usage examples

**Allocate, set, and get bytes:**

```beskid
let buf := Bytes.New(4)
buf.Set(0, 72)   // 'H'
buf.Set(1, 105)  // 'i'
buf.Set(2, 33)   // '!'
let first := buf.Get(0)  // 72
let third := buf.Get(2)  // 33
```

**Copy between buffers:**

```beskid
let src := Bytes.New(3)
src.Set(0, 10)
src.Set(1, 20)
src.Set(2, 30)

let dst := Bytes.New(3)
Bytes.Copy(dst, 0, src, 0, 3)  // dst now contains [10, 20, 30]
```

**Compare lexicographically:**

```beskid
let a := Bytes.New(2)
a.Set(0, 0x41)  // 'A'
a.Set(1, 0x42)  // 'B'

let b := Bytes.New(2)
b.Set(0, 0x41)  // 'A'
b.Set(1, 0x43)  // 'C'

let cmp := Bytes.Compare(a, b)  // negative (a < b lexicographically)
```

**FromString and SubSlice:**

```beskid
let data := Bytes.FromString("hello world")
let greeting := data.SubSlice(0, 5)  // "hello"
let space := data.Get(5)             // 32 (space)
```

## Gotchas

- **No resize or grow**: buffers are fixed-size after `New(len)`. There is no append, shrink, or realloc — allocate the exact size you need upfront.
- **OOB traps**: `Get` and `Set` with an index >= `Len` cause a runtime trap. Always guard with a bounds check or ensure the index is valid.
- **Allocation required upfront**: every buffer must be created via `New(len)` before use. There is no zero-length literal syntax or inline initializer for `u8[]`.
