`Core.Encoding` provides shared `EncodingError`, an `Encoder` contract, and **Utf8**, **Hex**, and **Base64** implementations.

## Modules

| Module | Role |
|--------|------|
| `Core.Encoding.Utf8` | Language-default UTF-8 encode/decode with validation |
| `Core.Encoding.Hex` | Lowercase hex encode; case-insensitive decode |
| `Core.Encoding.Base64` | RFC 4648 standard alphabet with padding |

Invalid input returns `Result::Error` — no silent replacement in v1.
