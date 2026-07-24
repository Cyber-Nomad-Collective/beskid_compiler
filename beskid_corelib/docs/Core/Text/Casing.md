# Core.Text.Casing

`Core.Text.Casing` converts identifier casing conventions: `snake_case`, `PascalCase`, `camelCase`, and prefixed callable names. It operates on ASCII ranges only (not Unicode-aware) and is designed for code generation and tooling.

## Functions

| Function | Signature | Behavior |
|----------|-----------|----------|
| `SnakeToPascal` | `string SnakeToPascal(string snake)` | `lower_run` → `LowerRun`. Underscores mark word boundaries. |
| `PascalToSnake` | `string PascalToSnake(string pascal)` | `LowerRun` → `lower_run`. Uppercase letters become `_lower`. |
| `SnakeToCamel` | `string SnakeToCamel(string snake)` | `lower_run` → `lowerRun`. Same as Pascal but lowercases the first character. |
| `CamelToSnake` | `string CamelToSnake(string camel)` | `lowerRun` → `lower_run`. Uppercase letters become `_lower`. |
| `CallableFromSnake` | `string CallableFromSnake(string snake, string prefix)` | `lower_run` with prefix `Parse` → `ParseLowerRun`. |
| `IsAsciiLower` | `bool IsAsciiLower(u8 b)` | Checks ASCII `a`–`z`. |
| `IsAsciiUpper` | `bool IsAsciiUpper(u8 b)` | Checks ASCII `A`–`Z`. |
| `IsAsciiDigit` | `bool IsAsciiDigit(u8 b)` | Checks ASCII `0`–`9`. |
| `IsSnakePartChar` | `bool IsSnakePartChar(u8 b, bool first)` | Validates a character for snake_case identifiers. |

## Conversion examples

```beskid
Casing.SnakeToPascal("parse_lower_run");
// → "ParseLowerRun"

Casing.PascalToSnake("ParseLowerRun");
// → "parse_lower_run"

Casing.SnakeToCamel("parse_lower_run");
// → "parseLowerRun"

Casing.CamelToSnake("parseLowerRun");
// → "parse_lower_run"

Casing.CallableFromSnake("lower_run", "Parse");
// → "ParseLowerRun"
```

## Common patterns

**Generating function names from schema field names:**

```beskid
string fieldName = "user_id";
string getter = Casing.CallableFromSnake(fieldName, "Get");
// getter == "GetUserId"
```

**Validating identifier parts:**

```beskid
bool validStart = Casing.IsSnakePartChar(byte, true);
bool validBody = Casing.IsSnakePartChar(byte, false);
```

## Gotchas

- ASCII-only. Non-ASCII characters pass through unmodified in `SnakeToPascal`/`PascalToSnake` — uppercase conversion relies on `b - 32` for `a`–`z` range only.
- `CamelToSnake` and `PascalToSnake` have identical logic today (both split on uppercase). Use the one that matches your semantic intent.
- Consecutive underscores in `SnakeToPascal` cause consecutive uppercase flips (each `_` sets `upperNext = true`), which may produce unexpected results for `__double_underscore` input.
