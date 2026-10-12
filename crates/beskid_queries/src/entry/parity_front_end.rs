//! Parity of the query gate with the obligations the retired `beskid_analysis` `TypeChecker`
//! judged on the executable path (`docs/design/2026-10-12-query-front-end.md`, section 5).
//! Each case was a `beskid_analysis` integration test that drove the legacy checker directly;
//! here the query gate alone judges the same programs. Two legacy cases are not ported by design:
//! a private runtime builtin (`memory_set`) called from user code is E1101 in the query gate, and an
//! integer constant returned as `pointer` is not judged because `pointer` is the ABI of every
//! nominal value (`legality::typing::compatible`).

use super::parity_harness::{assert_covered_kinds, assert_silent, codes, reports_kind, unit_findings};

const BASE: u64 = 7_100;

#[test]
fn legacy_executable_front_end_acceptances_stay_silent() {
    assert_silent(
        &[
            (
                "string ordering comparisons type as bool",
                "bool Main(string a, string b) { bool lt = a < b; bool lte = a <= b; bool gt = a > b; \
                 bool gte = \"Z\" >= \"a\"; return lt && lte && gt && gte; }",
            ),
            (
                "pointer and word are real primitive signature types",
                "pub pointer Identity(pointer value) { return value; } \
                 pub word Advance(word value) { return value + 1; } \
                 pub bool Same(pointer left, pointer right) { return left == right; }",
            ),
            (
                "a module integer constant is a literal fact",
                "const Capacity = 32; pub word Size() { return Capacity; }",
            ),
            (
                "a bare sibling method call beats a same-named free function",
                "pub type Counter { i64 value, pub i64 Add(i64 amount) { return value + amount; } \
                 pub i64 Twice(i64 amount) { return Add(Add(amount)); } } \
                 pub i64 Add(Counter counter, i64 amount, i64 extra) { return counter.value + amount + extra; }",
            ),
            (
                "a free function stays callable by module path and outside methods",
                "mod Tools { pub i64 Add(Counter counter, i64 amount, i64 extra) { return counter.value + amount + extra; } } \
                 pub type Counter { i64 value, pub i64 Add(i64 amount) { return value + amount; } \
                 pub i64 Both(i64 amount) { return Tools.Add(this, Add(amount), 1); } } \
                 pub i64 Add(Counter counter, i64 amount, i64 extra) { return counter.value + amount + extra; } \
                 pub i64 UseFree() { Counter counter = Counter { value: 1 }; return Add(counter, 2, 3); }",
            ),
            (
                "a bare sibling method in a generic type dispatches on the receiver",
                "pub type Holder<T> { T item, pub bool Has() { return true; } pub bool Check() { return Has(); } }",
            ),
            (
                "bare receiver field reads resolve in type-body methods",
                "pub type Port { u8 high, u8 low, pub i64 Value() { return i64(high) * 256_i64 + i64(low); } }",
            ),
        ],
        BASE,
    );
}

#[test]
fn legacy_executable_front_end_rejections_are_reported() {
    assert_covered_kinds(
        &[
            (
                "a string ordered against a number",
                "TypeMismatch",
                "E1206",
                "bool Main(string a, i64 n) { return a < n; }",
            ),
            (
                "an unknown bare call inside a method",
                "ResolveUnknownValue",
                "E1101",
                "pub type Counter { i64 value, pub i64 Get() { return Missing(); } }",
            ),
            (
                "a missing accessible struct field",
                "TypeMissingStructField",
                "E1212",
                "pub type Pair { pub i64 left, pub i64 right } i64 Main() { Pair pair = Pair { left: 1_i64 }; return pair.left; }",
            ),
        ],
        BASE + 100,
    );
}

#[test]
fn dependency_signatures_use_their_own_same_leaf_type() {
    let findings = unit_findings(
        &[
            (
                "Main.bd",
                "use Probe.Bsol.Schema; use Probe.Text.Regex.Bounded; \
                 unit Main() { Schema.PatternLimits policy = Schema.PatternLimits { depth: 128_i64 }; Schema.Validate(policy); return; }",
            ),
            (
                "Probe/Text/Regex/Bounded.bd",
                "pub type PatternLimits { pub i64 work, } pub i64 Read(PatternLimits policy) { return policy.work; }",
            ),
            (
                "Probe/Bsol/Schema.bd",
                "use Probe.Text.Regex.Bounded; pub type PatternLimits { pub i64 depth, } \
                 pub i64 Validate(PatternLimits policy) { return policy.depth; }",
            ),
        ],
        BASE + 300,
    );
    assert!(findings.is_empty(), "each signature keeps its lexical type declaration: {:?}", codes(&findings));
}

const NODES: &str = "pub type NodeRef { pub u64 issuer, pub u64 node, } pub type NodeSpan { pub u64 start, pub u64 end, }";
const QUERY: &str = "use Probe.Syntax.Nodes; pub type SyntaxQuery { pub Probe.Syntax.Nodes.NodeRef start, pub u64 bound, } \
     pub SyntaxQuery At(Probe.Syntax.Nodes.NodeRef root) { return SyntaxQuery { start: root, bound: 1_u64 }; } \
     pub Probe.Syntax.Nodes.NodeSpan Span(Probe.Syntax.Nodes.NodeRef node) { return Probe.Syntax.Nodes.NodeSpan { start: node.node, end: node.node }; }";

#[test]
fn an_imported_function_rejects_a_record_argument_of_the_wrong_nominal_type() {
    let findings = unit_findings(
        &[
            (
                "Main.bd",
                "use Probe.Compiler.Query; use Probe.Syntax.Nodes; \
                 pub u64 Main(NodeRef root) { NodeSpan span = Query.Span(Query.At(root)); return span.start; }",
            ),
            ("Probe/Syntax/Nodes.bd", NODES),
            ("Probe/Compiler/Query.bd", QUERY),
        ],
        BASE + 400,
    );
    assert!(
        reports_kind(&findings, "TypeCallArgumentMismatch", "E1205", Some(0)),
        "a SyntaxQuery argument for an imported NodeRef parameter is E1205: {:?}",
        codes(&findings)
    );
}

#[test]
fn an_imported_function_accepts_the_declared_nominal_argument() {
    let findings = unit_findings(
        &[
            (
                "Main.bd",
                "use Probe.Compiler.Query; use Probe.Syntax.Nodes; \
                 pub u64 Main(NodeRef root) { NodeSpan span = Query.Span(root); return span.start + Query.At(root).bound; }",
            ),
            ("Probe/Syntax/Nodes.bd", NODES),
            ("Probe/Compiler/Query.bd", QUERY),
        ],
        BASE + 500,
    );
    assert!(findings.is_empty(), "the declared NodeRef argument type-checks: {:?}", codes(&findings));
}
