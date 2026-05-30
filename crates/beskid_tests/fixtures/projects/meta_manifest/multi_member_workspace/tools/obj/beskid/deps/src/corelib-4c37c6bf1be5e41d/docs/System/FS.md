`System.FS` defines **`FsError`** and **`ReadAllText`** / **`WriteAllText`** returning **`Result<_, FsError>`**.

## FsError

```beskid
pub enum FsError {
    NotFound(string path),
    PermissionDenied(string path),
    Unknown(string message),
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `ReadAllText(string path)` | Empty path → **`NotFound`**. Otherwise **`Unknown("not implemented")`** today. |
| `WriteAllText(string path, string text)` | Empty path → **`NotFound`**. Empty `text` → **`Ok(true)`**. Non-empty text → **`Unknown("not implemented")`**. |
| `Exists(string path)` | Returns **`path != ""`** (placeholder existence check). |

Treat this module as API surface + error taxonomy until real host FS hooks land.
