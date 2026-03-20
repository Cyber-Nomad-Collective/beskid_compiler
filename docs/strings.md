# Runtime Strings (v1.0)

## Contracts
- `str_new(ptr, len)` requires:
  - `ptr != null`
  - bytes `[ptr, ptr+len)` are valid UTF-8
- Invalid UTF-8 panics with `invalid utf-8 string data`.
- Null input panics with `null string data`.

## Operations
- `str_len` returns byte length.
- `str_concat` allocates a new buffer and copies left/right bytes in order.
- Null handles panic with `null string handle`.

## Notes
- String data is byte-oriented for ABI stability.
- Character indexing is language-level, not runtime builtin-level.
