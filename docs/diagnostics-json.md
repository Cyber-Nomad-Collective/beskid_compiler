# Diagnostics JSON format

`beskid check`, `build`, `run`, `test`, `fmt` and `dev build` accept `--error-format=json`. With it, the command writes each compiler diagnostic to stderr as one JSON object on one line, instead of the drawn report. Other output (progress, summaries, `--json` result records on stdout) does not change.

The format is modelled on rustc's JSON diagnostics. It is produced from the one structured diagnostic (`beskid_analysis::analysis::diagnostics::json`), after spans in materialized copies under `obj/beskid/` are mapped back to the authored files and repeated causes are removed.

## Stability

- `schema` is the schema version, now `1`. It changes only when a field changes meaning or is removed.
- Consumers must accept unknown fields and unknown enum values. New fields and new values can appear without a version change.
- One line holds one object. Do not depend on the order of fields.

## Diagnostic object

| Field | Type | Meaning |
|---|---|---|
| `$message_type` | string | Always `"diagnostic"`. |
| `schema` | number | Schema version (`1`). |
| `message` | string | One sentence that names the cause. |
| `code` | object or null | `{ "code": "E1214", "explanation": null, "url": "https://..." }`. `url` is the diagnostic-code registry page for registry codes, else null. `explanation` is reserved and null. |
| `level` | string | `"error"`, `"warning"` or `"note"`. |
| `origin` | string or null | `"beskid:mod:<type id>"` for a diagnostic from a Mod analyzer, else null. |
| `spans` | array of span | The primary span first (`is_primary: true`), then one span per related location (`is_primary: false`) with its label. |
| `children` | array of child | Help and fixes (see below). |
| `tags` | array of string | `"unnecessary"` (unused or unreachable code) and `"deprecated"`. |
| `rendered` | string or null | The drawn report of the same diagnostic. |

A failure that has no structured diagnostic (for example a manifest that cannot be read) is written as one object with `code: null`, `level: "error"` and empty `spans`, `children` and `tags`.

## Span object

| Field | Type | Meaning |
|---|---|---|
| `file_name` | string | Authored file of the span. |
| `byte_start`, `byte_end` | number | Byte offsets in the file, end exclusive. |
| `line_start`, `line_end` | number | 1-based lines. |
| `column_start`, `column_end` | number | 1-based columns, counted in Unicode scalar values. |
| `is_primary` | bool | `true` for the primary span and for fix edits. |
| `label` | string or null | Short label of the place ("assigned again"). |
| `suggested_replacement` | string or null | For a fix edit: the replacement text. An empty span inserts it. |
| `suggestion_applicability` | string or null | For a fix edit: `"MachineApplicable"`, `"MaybeIncorrect"` or `"HasPlaceholders"`. |

## Child object

Children have the fields `message`, `code` (null), `level` (`"help"`), `spans`, `children` (empty) and `rendered` (null).

- Help: `message` is the help sentence and `spans` is empty.
- Fix: `message` is the fix title and `spans` holds its edits, each with `suggested_replacement` and `suggestion_applicability`. Apply a fix without review only when its applicability is `MachineApplicable`.

## Example

```json
{"$message_type":"diagnostic","schema":1,"message":"Unused import `A.B`.","code":{"code":"W1503","explanation":null,"url":"https://beskid-lang.org/docs/standard/capabilities/compiler--semantic-pipeline--diagnostic-code-registry/"},"level":"warning","origin":null,"spans":[{"file_name":"Src/Main.bd","byte_start":4,"byte_end":7,"line_start":1,"line_end":1,"column_start":5,"column_end":8,"is_primary":true,"label":"never used","suggested_replacement":null,"suggestion_applicability":null}],"children":[{"message":"Remove the import","code":null,"level":"help","spans":[{"file_name":"Src/Main.bd","byte_start":0,"byte_end":9,"line_start":1,"line_end":2,"column_start":1,"column_end":1,"is_primary":true,"label":null,"suggested_replacement":"","suggestion_applicability":"MachineApplicable"}],"children":[],"rendered":null}],"tags":["unnecessary"],"rendered":"..."}
```
