# Core.Text.Cursor

`Core.Text.Cursor` exposes a byte-indexed UTF-8 cursor type (`TextCursor`) for incremental string traversal. It is the foundational position-tracking layer used by parser combinators (`Core.Text.Parser`) and regex matching (`Core.Text.Regex`).

## Type

```beskid
pub type TextCursor {
    string source,
    i64 pos,
}
```

## Functions

| Function | Signature | Behavior |
|----------|-----------|----------|
| `From` | `TextCursor From(string source)` | Creates a cursor at position `0`. |
| `Len` | `i64 Len(TextCursor c)` | Total UTF-8 code units in the source string. |
| `Position` | `i64 Position(TextCursor c)` | Current byte offset. |
| `AtEnd` | `bool AtEnd(TextCursor c)` | `true` when `pos >= Len(c)`. |
| `Slice` | `string Slice(TextCursor c, i64 start, i64 count)` | Extract `count` bytes starting at `start`. Returns `""` when `count < 1` or `start < 0`. |
| `Remaining` | `string Remaining(TextCursor c)` | Everything from `pos` to end. |
| `DropSource` | `string DropSource(TextCursor c, i64 count)` | The source with `count` leading bytes removed (returns original if `count < 1`). |
| `Advance` | `TextCursor Advance(TextCursor c, i64 count)` | Moves `pos` forward by `count`, clamped to `[0, Len(c)]`. |
| `StartsWith` | `bool StartsWith(TextCursor c, string prefix)` | `true` when the remaining text begins with `prefix`. |
| `IndexOfFrom` | `i64 IndexOfFrom(TextCursor c, i64 start, string needle)` | Searches for `needle` from `pos + start` onward. |
| `ContainsSubstring` | `bool ContainsSubstring(TextCursor c, string needle)` | `true` when `needle` appears anywhere after `pos`. |

## Basic traversal

```beskid
TextCursor c = Cursor.From("hello world");

while !Cursor.AtEnd(c) {
    string ch = Cursor.Slice(c, Cursor.Position(c), 1);
    // process ch
    c = Cursor.Advance(c, 1);
}
```

## Checking a prefix

```beskid
TextCursor c = Cursor.From("fn main() { }");

if Cursor.StartsWith(c, "fn") {
    // advance past the keyword
    c = Cursor.Advance(c, 2);
}
```

## Getting remaining text

```beskid
TextCursor c = Cursor.From("abc123");
c = Cursor.Advance(c, 3);
string rest = Cursor.Remaining(c);
// rest == "123"
```

## Common patterns

**Cursor inside a parsing loop:**

```beskid
TextCursor c = Cursor.From(input);
while !Cursor.AtEnd(c) {
    if Cursor.StartsWith(c, "//") {
        // skip comment line
        c = Cursor.Advance(c, Cursor.Len(c));
        break;
    }
    c = Cursor.Advance(c, 1);
}
```

## Gotchas

- All positions are **byte offsets**, not Unicode code points. Multi-byte UTF-8 sequences span multiple positions. The cursor is designed for grammar-driven parsing where you advance by known token lengths, not for generic grapheme-cluster traversal.
- `Slice` does not bounds-check the `start` range beyond `start < 0` — it delegates to `__str_slice`.
- `Advance` with a negative count clamps to `0`; with a count past end it clamps to `Len(c)`.
- `Cursor` is imported as `Core.Text.Cursor` — it is **not** re-exported from `Prelude.bd`.
