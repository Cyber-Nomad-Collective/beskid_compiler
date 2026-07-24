`Core.Encoding` provides shared `EncodingError`, an `Encoder` contract, and **Utf8**, **Hex**, and **Base64** implementations.

## Modules

| Module | Role |
|--------|------|
| `Core.Encoding.Utf8` | Language-default UTF-8 encode/decode with validation |
| `Core.Encoding.Hex` | Lowercase hex encode; case-insensitive decode |
| `Core.Encoding.Base64` | RFC 4648 standard alphabet with padding |

Invalid input returns `Result::Error` — no silent replacement in v1.

## Usage examples

Hex encode → decode roundtrip:

```beskid
let original = "hello";
let encoded = Core.Encoding.Hex.Encode(original);
// encoded == "68656c6c6f"
let decoded = Core.Encoding.Hex.Decode(encoded);
// decoded == Ok("hello")
```

Base64 encode:

```beskid
let input = "BeskiD";
let b64 = Core.Encoding.Base64.Encode(input);
// b64 == "QmVza2lE"
```

Utf8 validate:

```beskid
match Core.Encoding.Utf8.Validate(bytes) {
  case Ok:    /* bytes are valid UTF-8 */
  case Error: /* handle EncodingError */
}
```

## Gotchas

- **No URL-safe Base64.** The encoder always produces the standard RFC 4648 alphabet with `+` and `/` — there is no URL-safe `-`/`_` variant.
- **Hex decode is case-insensitive, encode is lowercase-only.** `Hex.Decode("A")` and `Hex.Decode("a")` both succeed, but `Hex.Encode` always emits `"a"`–`"f"`.
