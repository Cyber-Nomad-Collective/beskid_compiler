`System.Path` provides **`Combine`**, **`FileName`**, and **`Extension`**.

## Functions

| Function | Behavior |
|----------|----------|
| `Combine(string left, string right)` | If `left` is empty returns `right`; if `right` is empty returns `left`; otherwise **`left + "/" + right`**. |
| `FileName(string path)` | Returns **`path`** unchanged (no parsing yet). |
| `Extension(string path)` | Returns **`path`** unchanged (no parsing yet). |

Use **`Combine`** for safe manual joins; do not rely on **`FileName`** / **`Extension`** until they parse platform paths.
