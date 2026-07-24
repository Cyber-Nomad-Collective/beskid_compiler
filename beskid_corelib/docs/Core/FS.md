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

## Usage examples

### ReadAllText

```beskid
let content = FS.ReadAllText("/tmp/hello.txt");
match content {
    Ok(text) => Console.WriteLine($"file says: {text}"),
    Err(FsError.NotFound(path)) => Console.WriteLine($"not found: {path}"),
    Err(other) => Console.WriteLine($"read failed: {other}"),
}
```

### WriteAllText

```beskid
let lines = "hello\nworld\n";
match FS.WriteAllText("/tmp/out.txt", lines) {
    Ok(()) => Console.WriteLine("wrote file"),
    Err(e) => Console.WriteLine($"write failed: {e}"),
}
```

### Exists + conditional read

```beskid
let path = "/etc/config.json";
if FS.Exists(path) {
    match FS.ReadAllText(path) {
        Ok(data) => parse_config(data),
        Err(_) => Console.WriteLine("exists but unreadable"),
    }
} else {
    Console.WriteLine($"{path} missing, using defaults");
}
```

### CreateDirectory + WriteAllText

```beskid
match FS.CreateDirectory("/tmp/app/cache") {
    Ok(()) | Err(FsError.AlreadyExists(_)) => {
        let _ = FS.WriteAllText("/tmp/app/cache/info.txt", "ready");
    },
    Err(e) => Console.WriteLine($"mkdir failed: {e}"),
}
```

## Gotchas

- **No recursive directory creation.** `CreateDirectory("/a/b/c")` fails if `/a/b` does not exist. Create each parent first.
- **No append mode.** `WriteAllText` overwrites the file; use `ReadAllText` + concatenation + `WriteAllText` for append-like behaviour.
- **Text-only.** For binary I/O, use `Core.Syscall.ReadBytes` / `WriteBytes` instead.
