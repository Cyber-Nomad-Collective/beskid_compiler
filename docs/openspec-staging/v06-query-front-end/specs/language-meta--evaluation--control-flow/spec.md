## MODIFIED Requirements

### Requirement: Conditional and loop statement typing
`if (cond) block else block` and `while (cond) block` MUST require `cond` to be `bool`. `for id in expr block` MUST require `expr` to be iterable: the built-in `range(start, end)` form, a value whose type is an array `T[]`, or a nominal value whose type declares the iterator protocol (`Option<T> Next()`); any other iterable is **E1215**. `return expr?;` MUST return from the innermost function and `expr` MUST match the return type when present.

#### Scenario: Non-bool if condition
- **GIVEN** an `if` whose condition expression is not `bool`
- **WHEN** control-flow typing runs
- **THEN** the compiler emits a non-bool condition diagnostic

#### Scenario: For over a non-iterable value
- **GIVEN** `unit Main(i64 n) { for i in n { return; } return; }`
- **WHEN** control-flow typing runs
- **THEN** the compiler SHALL report **E1215** at the iterable

## ADDED Requirements

### Requirement: Array iteration
`for id in expr block` where `expr` has type `T[]` SHALL evaluate `expr` exactly once, before the first iteration, to an array value; a null array value SHALL trap as an out-of-bounds access does. The loop SHALL run one iteration per index from `0` to the array's length (exclusive), in increasing order; at the start of each iteration `id` SHALL be bound to the element at the current index, read from the array value evaluated at loop entry. `id` SHALL have type `T` and SHALL be immutable (assigning it is **E1214**). Stores into not-yet-visited elements of the same array value SHALL be observed by later iterations (arrays are reference values); rebinding the local that named the iterable SHALL NOT change the iteration. `break` and `continue` SHALL target the loop. When `T` is a managed type the loop variable SHALL be a garbage-collector root for the duration of its iteration.

#### Scenario: Summing an array parameter
- **GIVEN** `i64 Sum(i64[] values) { mut i64 total = 0_i64; for value in values { total = total + value; } return total; }`
- **WHEN** the program is built and `Sum([1_i64, 2_i64, 3_i64])` runs
- **THEN** the compiler SHALL accept the program without a diagnostic
- **AND** the call SHALL return `6`

#### Scenario: Managed elements
- **GIVEN** `string Last(string[] names) { mut string last = ""; for name in names { last = name; } return last; }`
- **WHEN** the program is built
- **THEN** the loop variable `name` SHALL be a rooted managed local for each iteration
- **AND** `Last(["a", "b"])` SHALL return `"b"`

#### Scenario: Assigning the loop variable
- **GIVEN** `unit Main(i64[] values) { for value in values { value = 1_i64; } return; }`
- **WHEN** `beskid check` judges `Main`
- **THEN** the compiler SHALL report **E1214** at the assignment
