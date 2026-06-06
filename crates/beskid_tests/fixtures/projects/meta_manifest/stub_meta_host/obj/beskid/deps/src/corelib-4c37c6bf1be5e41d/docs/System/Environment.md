`System.Environment` defines **`EnvironmentError`** and accessors backed by runtime `env_*` builtins.

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
| `Get(string name)` | Empty name → **`InvalidName`**. Lookup via **`__env_get`**; unset → **`NotFound`**. |
| `TryGet(string name)` | Empty name → **`None`**. Unset → **`None`**; otherwise **`Some(value)`**. |
| `Set(string name, string value)` | Empty name → **`InvalidName`**. Mutation via **`__env_set`**; unsupported host → **`UnsupportedMutation`**. |
| `GetVariable` | Alias of **`Get`**. |
| `CurrentDirectory()` | Returns **`__env_getcwd()`**. |
