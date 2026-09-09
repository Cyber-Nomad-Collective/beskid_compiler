# Grouped-expression and numeric-operator semantics

Date: 2026-09-08

## Question

How should beskid preserve type identity through parenthesized expressions, infer integer literals in typed binary expressions, and select signed or unsigned lowering for arithmetic and comparisons?

The immediate regression is representative rather than special:

```beskid
word Main(word tail) {
    word nextTail = (tail + 1) % 32;
    return nextTail;
}
```

The outer `%` must observe the left operand as `word`, even though its syntax node is a grouped-expression wrapper around a nested binary expression. Both literal operands must be contextualized as `word`, and lowering must select unsigned remainder.

## Primary-source findings

| Language | Grouped-expression identity | Literal typing | Operator and comparison typing |
|---|---|---|---|
| Swift | The language reference states directly that grouping parentheses do not change an expression's type; `(1)` remains `Int`. A one-element parenthesized expression is distinct from a tuple. [Swift Expressions](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/expressions/#Parenthesized-Expression) | An explicit annotation can determine an integer literal's type, while an unconstrained integer literal defaults to `Int`. [Swift Lexical Structure](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/lexicalstructure/#Literals), [Swift Basics](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/thebasics/#Type-Safety-and-Type-Inference) | Swift does not implicitly convert stored values between numeric types; operations across integer types require an explicit conversion. `UInt` comparison operators have signatures such as `(UInt, UInt) -> Bool`, so signedness is a property of the resolved operand type. [Swift Basics: Integer Conversion](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/thebasics/#Integer-Conversion), [Swift `UInt`](https://developer.apple.com/documentation/swift/uint) |
| Go | Parentheses are part of `PrimaryExpr` syntax and only regroup an expression; the spec explicitly treats some operands as “possibly parenthesized” without changing their semantic category. [Go specification: Primary expressions](https://go.dev/ref/spec#Primary_expressions), [Go specification: Address operators](https://go.dev/ref/spec#Address_operators) | Untyped constants remain exact constants through constant expressions. In a non-shift binary operation with one typed operand, an untyped constant is converted to that operand's type. [Go specification: Operators](https://go.dev/ref/spec#Operators), [Go specification: Constant expressions](https://go.dev/ref/spec#Constant_expressions) | Non-constant binary operands must have identical types (apart from specified exceptions). Comparisons require assignability in one direction and return an untyped boolean; integer operands are compared according to their resolved integer types. [Go specification: Operators](https://go.dev/ref/spec#Operators), [Go specification: Comparison operators](https://go.dev/ref/spec#Comparison_operators) |
| Mojo | The expression reference describes `(x)` as “just x”; a comma, not parentheses, creates a tuple. [Mojo expression reference](https://mojolang.org/docs/reference/expressions/#parenthesized-expressions) | A bare integer is an arbitrary-precision `IntLiteral` until context materializes it. Beside a typed value, the literal takes that value's type; without context it materializes to word-sized `Int`. [Mojo numeric types](https://mojolang.org/docs/reference/numeric-types/#numeric-literals) | Numeric variables do not widen or narrow implicitly; explicit conversion is required. Operators over concrete SIMD/scalar operands require matching element type, and a mismatch is a compile-time error. [Mojo types](https://mojolang.org/docs/manual/types/#numeric-type-conversion), [Mojo operators](https://mojolang.org/docs/manual/operators/#arithmetic-and-bitwise-operators) |
| Rust | A grouped expression evaluates to its enclosed operand and preserves whether that operand is a place or value expression. [Rust Reference: Grouped expressions](https://doc.rust-lang.org/reference/expressions/grouped-expr.html) | A suffixed literal has the named integer type. An unsuffixed literal takes the uniquely inferred surrounding integer type, defaults to `i32` only when under-constrained, and is an error when over-constrained. [Rust Reference: Integer literals](https://doc.rust-lang.org/reference/expressions/literal-expr.html#integer-literal-expressions) | Arithmetic and comparison dispatch is defined on the resolved operand types. Signedness affects observable lowering semantics, including arithmetic versus logical right shift; comparison overloads resolve through the operand types. [Rust Reference: Operator expressions](https://doc.rust-lang.org/reference/expressions/operator-expr.html#arithmetic-and-logical-binary-operators) |

## Synthesis

All four systems separate three concerns:

1. **Grouping changes syntax precedence, not value/type identity.** A grouped-expression node is a transparent semantic projection of its enclosed expression.
2. **Literal typing is resolved before machine lowering.** Literal syntax either remains provisional until context supplies a type (Go, Mojo, Rust, Swift) or receives a language default only when genuinely unconstrained.
3. **Signedness is selected from resolved source types.** Width alone is insufficient: signed and unsigned integers can share a machine representation while requiring different division, remainder, right-shift, extension, and relational comparison instructions.

The current beskid failure is consistent with violating item 1 at the fact boundary. The semantic-expression helper already recursively preserves grouped-expression identity, but syntax facts are queried by concrete `AstNodeKey`. If the wrapper key has no ABI/semantic projection, the outer binary operator cannot establish symmetric signedness and correctly fails closed with `MissingRuleOrFact`.

## Recommended beskid design

### 1. One transparent-wrapper projection

Define a single semantic-contract operation for transparent expression wrappers:

```text
transparent expression key -> enclosed value-expression key
```

Use it as the shared normalization step for semantic type, ABI type, managed-reference kind, constant value, and contextual-literal queries. Do not duplicate grouped-expression tree walking in each consumer. The projection must preserve generation/unit identity and fail closed if the expected direct-child relationship is absent.

This should be extensible to any future wrapper that is semantically transparent, but the allowed wrapper kinds must be explicit; casts, conversions, `try`, and blocks are not automatically transparent.

### 2. Resolve a typed binary contract before ISLE lowering

Publish one authoritative fact for every accepted binary expression:

```text
TypedBinaryOperation {
    left_type,
    right_type,
    result_type,
    numeric_domain,
    signedness,
    conversion_intents
}
```

The semantic layer should first contextualize provisional integer literals from the non-literal operand or expected result type, verify representability, and then require compatible concrete operand types. A missing or contradictory type must remain a semantic error; lowering must not guess from CLIF widths or default an unresolved operand to signed.

### 3. Make lowering consume the operation contract

ISLE/Cranelift selection should consume `numeric_domain` and `signedness` from that resolved fact for all sign-sensitive families:

- `div`, `%`
- ordered integer comparisons
- right shift
- widening extension

Equality does not require signed versus unsigned condition codes, but it should still require the same compatible-operand proof so equality cannot become a separate permissive path.

### 4. Keep conversion explicit and symmetric

Follow Swift/Mojo's fail-closed rule for concrete mixed numeric types: do not silently merge `i32` and `u32`, or `word` and `i64`. Context may materialize an unsuffixed literal into the peer type when representable; two already-concrete incompatible operands require an explicit cast. Apply this symmetrically regardless of which side contains the literal.

### 5. Test the contract as a matrix

Add semantic-fact and emitted-CLIF tests for each sign-sensitive operator with:

- ungrouped and multiply nested grouped operands;
- contextual literal on the left and on the right;
- `word`, `u32`, `u8`, `i32`, and `i64` where supported;
- boundary values that distinguish zero extension from sign extension;
- unsigned values above the signed maximum for comparisons, division, and remainder;
- incompatible concrete signed/unsigned pairs that must fail before lowering;
- deliberately missing wrapper facts that must report the wrapper/expression source span.

The release regression should assert both the semantic fact (`GroupedExpression -> word`) and `urem` emission for `(tail + 1) % 32`. Parallel tests should cover unsigned relational condition codes so a future wrapper fix cannot leave comparison semantics divergent from remainder semantics.

## Decision

Treat grouping as an atomized, transparent semantic-contract projection and operator selection as a consumer of a resolved typed-binary fact. This matches the shared design of Swift, Go, Mojo, and Rust while preserving beskid's fail-closed rule: syntax wrappers may forward established identity, but lowering may never invent it.
