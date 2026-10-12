//! `beskid check` obligation for string concatenation and interpolation operands without a text
//! form (E1209).

use beskid_analysis::analysis::SemanticIssueKind;
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{GateObligationKind, check_gate_obligations, gate_obligations};

use super::support::{key, setup};

#[test]
fn interpolating_a_nominal_value_is_reported_once_at_the_operand() {
    let source = "type Point { i64 x } string Main(Point p, i64 n, bool b, f64 f) { return \"p ${p} ${n} ${b} ${f}\"; }";
    let (db, _project, unit, generation, index) = setup(source);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let obligations = gate_obligations(&db, main).expect("gate query").expect("gate obligations");
    let findings = obligations
        .iter()
        .filter(|obligation| matches!(obligation.kind, GateObligationKind::StringConcatOperandNotConvertible { .. }))
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 1, "only the nominal operand has no text form: {obligations:?}");
    let GateObligationKind::StringConcatOperandNotConvertible { found } = &findings[0].kind else { unreachable!() };
    assert!(found.ends_with("Point"), "the source spelling of the operand type: {found}");
    let rendered = check_gate_obligations(&db, &[main]);
    let finding = rendered
        .iter()
        .find(|finding| matches!(finding.kind, SemanticIssueKind::TypeStringConcatOperandNotConvertible { .. }))
        .expect("rendered E1209 finding");
    assert_eq!(finding.kind.code(), "E1209");
}

#[test]
fn concatenating_convertible_primitives_is_not_a_finding() {
    let source = "string Main(string s, i64 n, u8 b, bool flag, f32 f) { return s + n + b + flag + f; }";
    let (db, _project, unit, generation, index) = setup(source);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let obligations = gate_obligations(&db, main).expect("gate query").expect("gate obligations");
    assert!(
        !obligations
            .iter()
            .any(|obligation| matches!(obligation.kind, GateObligationKind::StringConcatOperandNotConvertible { .. })),
        "{obligations:?}"
    );
}
