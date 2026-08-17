`Core.Path` provides POSIX-oriented path helpers using **`Core.String`** and **`__str_slice`**.

## Functions

| Function | Behavior |
|----------|----------|
| `Combine(string left, string right)` | If `left` is empty returns `right`; if `right` is empty returns `left`; otherwise **`left + "/" + right`**. |
| `FileName(string path)` | Returns the segment after the final **`/`**, or the whole path when no separator is present. |
| `Extension(string path)` | Returns the substring after the first **`.`** in the file name, or **`""`**. |
| `IsAbsolute(string path)` | **`true`** when the path begins with **`/`** (POSIX v1). |
| `IsEmpty(string path)` | **`true`** when `path == ""`**. |
