`Core.FS` defines typed file helpers backed only by the canonical ABI-v5 `fs_*` adapters.

## FsError

```beskid
pub enum FsError {
    NotFound(string path),
    PermissionDenied(string path),
    IOError(string path),
    InvalidPath(string path),
    AlreadyExists(string path),
}
```

The boundary maps every `BeskidFsStatus` value exactly once. An unrecognized status fails closed as `IOError`; it is never interpreted as success.

## Functions

| Function | Result |
|----------|--------|
| `ReadAllText(string path)` | `Result<string, FsError>` |
| `WriteAllText(string path, string text)` | `Result<unit, FsError>` |
| `Delete(string path)` | `Result<unit, FsError>` |
| `CreateDirectory(string path)` | `Result<unit, FsError>` |
| `Exists(string path)` | `Result<bool, FsError>` |
| `Copy(string source, string destination)` | `Result<unit, FsError>` |

`ReadAllText` uses the adapter's output parameter only when the status is `Ok`. A successful empty-file read is therefore `Result::Ok("")`, distinct from every error.

`Exists` maps `Ok` to `Result::Ok(true)` and `NotFound` to `Result::Ok(false)`. Permission, I/O, invalid-input, already-exists, and unrecognized statuses remain typed failures.

## Usage examples

```beskid
use Core.FS;
use Core.FS.FsError;
use Core.Results;

match FS.ReadAllText("/tmp/hello.txt") {
    Result::Ok(text) => Console.WriteLine(text),
    Result::Error(FsError::NotFound(path)) => Console.WriteLine("missing: " + path),
    Result::Error(_) => Console.WriteLine("read failed"),
};
```

```beskid
match FS.WriteAllText("/tmp/out.txt", "") {
    Result::Ok(()) => Console.WriteLine("wrote empty file"),
    Result::Error(error) => Console.WriteLine("write failed"),
};
```

```beskid
match FS.Exists("/etc/config.json") {
    Result::Ok(true) => Console.WriteLine("present"),
    Result::Ok(false) => Console.WriteLine("missing"),
    Result::Error(error) => Console.WriteLine("existence check failed"),
};
```

## Constraints

- `CreateDirectory` is not recursive.
- `WriteAllText` overwrites the file; there is no append mode.
- The API is text-only. Binary I/O belongs to `Core.Syscall`.
- Corelib does not declare, emulate, or fall back from the manifest-owned adapters.
