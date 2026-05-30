`System.Environment` defines **`EnvironmentError`** and accessors for process environment data.

## EnvironmentError

```beskid
pub enum EnvironmentError {
    InvalidName(string name),
    NotFound(string name),
    UnsupportedMutation,
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `Get(string name)` | Empty name → **`InvalidName`**. Otherwise **`NotFound`** (no host lookup yet). |
| `TryGet(string name)` | Empty name → **`None`**. Otherwise **`None`**. |
| `Set(string name, string value)` | Empty name → **`InvalidName`**. Empty `value` or any mutation attempt → **`UnsupportedMutation`**. |
| `GetVariable` | Alias of **`Get`**. |
| `CurrentDirectory()` | Returns **`"."`**. |
