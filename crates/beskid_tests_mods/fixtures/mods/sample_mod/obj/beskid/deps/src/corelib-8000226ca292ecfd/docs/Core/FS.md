`Core.FS` defines **`FsError`** and file helpers backed by runtime `fs_*` builtins.

## FsError

```beskid
pub enum FsError {
    NotFound(string path),
    PermissionDenied(string path),
    AlreadyExists(string path),
    InvalidPath(string path),
    Unknown(string message),
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `ReadAllText(string path)` | Empty path → **`InvalidPath`**. Reads file text via **`__fs_read_text`**; missing file → **`NotFound`**. |
| `WriteAllText(string path, string text)` | Empty path → **`InvalidPath`**. Writes via **`__fs_write_text`**; I/O failure → **`Unknown`**. |
| `Delete(string path)` | Empty path → **`InvalidPath`**. Deletes via **`__fs_delete`**. |
| `CreateDirectory(string path)` | Empty path → **`InvalidPath`**. Creates via **`__fs_mkdir`**. |
| `Exists(string path)` | Returns **`__fs_exists(path) == 1`** for non-empty paths. |

Text paths use UTF-8 string handles; binary I/O uses **`Core.Syscall.ReadBytes`** / **`WriteBytes`**.
