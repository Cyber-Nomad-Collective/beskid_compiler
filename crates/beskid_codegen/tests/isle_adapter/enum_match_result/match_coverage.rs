//! Every `match` form the 0.6 surface admits lowers through the canonical TypedProgram ->
//! CodegenInput -> ISLE path, one form per test. Each program passes the legality gate first
//! (`lower_syntax_program` runs `check_items`), so a form `check` accepts but ISLE cannot lower
//! fails here as the original `MissingRuleOrFact` would.

use super::super::support::{
    SyntaxModuleItem, find_function_definitions, find_nodes_of_kind, item_fixture_with_root, item_name,
    lower_syntax_program,
};

const OPTION_RESULT: &str = "enum Option<T> { Some(T value), None } \
     enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } ";

/// Lower every function of `source` as one program and return each function's CLIF by name.
fn lower_program(label: &str, source: &str) -> Vec<(String, String)> {
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let items = find_function_definitions(db, root)
        .into_iter()
        .map(|key| SyntaxModuleItem { key, symbol: item_name(db, key).unwrap().unwrap().to_string() })
        .collect::<Vec<_>>();
    let artifact = match lower_syntax_program(&input, isa.as_ref(), &items) {
        Ok(artifact) => artifact,
        Err(error) => panic!("{label} must lower: {error}"),
    };
    artifact
        .functions
        .iter()
        .map(|function| (function.name.clone(), function.function.display().to_string()))
        .collect()
}

fn clif_of<'a>(functions: &'a [(String, String)], name: &str) -> &'a str {
    functions
        .iter()
        .find(|(symbol, _)| symbol.contains(name))
        .map(|(_, clif)| clif.as_str())
        .unwrap_or_else(|| panic!("no lowered function {name}: {:?}", functions.iter().map(|(n, _)| n).collect::<Vec<_>>()))
}

/// ProcessSuite `Fixture`: a value match on a call result whose error arm is a block with a call
/// and a `return`, so that arm diverges and supplies no merge value.
#[test]
fn value_match_with_a_diverging_block_arm_containing_a_call_and_return_lowers() {
    lower_program(
        "diverging block arm",
        &format!(
            "{OPTION_RESULT}Result<string, i64> Lookup(i64 key) {{ return Result::Error(key); }} \
             unit Note() {{ return; }} \
             string Fixture() {{ return match Lookup(1_i64) {{ \
             Result::Ok(path) => path, \
             Result::Error(_) => {{ Note(); return \"\"; }}, }}; }}"
        ),
    );
}

/// ProcessSuite `ErrorName`: a value match over a nominal enum with string, multi-field, and unit
/// variants, every arm a string literal.
#[test]
fn value_match_over_multi_field_and_unit_variants_lowers() {
    lower_program(
        "multi-field and unit variants",
        "enum ProcessError { NotFound(string executable), ExitCode(i64 code, string stderr), Closed, Busy } \
         string ErrorName(ProcessError error) { return match error { \
         ProcessError::NotFound(executable) => \"NotFound\", \
         ProcessError::ExitCode(code, stderr) => \"ExitCode\", \
         ProcessError::Closed => \"Closed\", \
         ProcessError::Busy => \"Busy\", }; }",
    );
}

/// ProcessSuite loop bodies: a statement match inside a loop whose arms are a unit call, an empty
/// block, and a block with a local, a nested `if`, and a call.
#[test]
fn statement_match_with_block_arms_inside_a_loop_lowers() {
    lower_program(
        "statement match in a loop",
        &format!(
            "{OPTION_RESULT}Result<i64, i64> Step(i64 index) {{ return Result::Ok(index); }} \
             unit Note() {{ return; }} \
             i64 Main() {{ mut i64 total = 0_i64; mut i64 index = 0_i64; \
             while (index < 4_i64) {{ \
             match Step(index) {{ \
             Result::Error(_) => Note(), \
             Result::Ok(value) => {{ i64 doubled = value * 2_i64; if (doubled > 2_i64) {{ total = total + doubled; }} Note(); }}, }}; \
             match Step(index) {{ Result::Ok(_) => {{ }}, Result::Error(error) => {{ total = total - error; }}, }}; \
             index = index + 1_i64; }} \
             return total; }}"
        ),
    );
}

/// The ProcessSuite `TryWrite` gap: a nested match whose scrutinee is the generic payload binding
/// `sent : Option<i64>` of an enclosing `Result<Option<i64>, i64>` match, as a statement and as a
/// value, plus the equivalent nested enum pattern.
#[test]
fn nested_match_on_a_generic_payload_binding_lowers() {
    let functions = lower_program(
        "nested generic payload match",
        &format!(
            "{OPTION_RESULT}Result<Option<i64>, i64> TryWrite(i64 count) {{ return Result::Error(count); }} \
             i64 Main() {{ mut i64 written = 0_i64; mut bool progressed = false; \
             match TryWrite(3_i64) {{ \
             Result::Error(_) => {{ }}, \
             Result::Ok(sent) => {{ \
             match sent {{ Option::Some(bytes) => {{ written = written + bytes; progressed = bytes > 0_i64; }}, Option::None => {{ }}, }}; \
             }}, }}; \
             i64 direct = match TryWrite(4_i64) {{ \
             Result::Ok(sent) => match sent {{ Option::Some(bytes) => bytes, Option::None => -1_i64, }}, \
             Result::Error(_) => -2_i64, }}; \
             i64 nested = match TryWrite(5_i64) {{ \
             Result::Ok(Option::Some(bytes)) => bytes, Result::Ok(Option::None) => 0_i64, Result::Error(code) => code, }}; \
             return written + direct + nested; }}"
        ),
    );
    let main = clif_of(&functions, "Main");
    assert!(main.contains("load.i64"), "the Option<i64> payload must be loaded as i64: {main}");
}

/// `break` and `continue` arms diverge in value position inside a loop.
#[test]
fn value_match_with_break_and_continue_arms_lowers() {
    lower_program(
        "break and continue arms",
        &format!(
            "{OPTION_RESULT}Result<i64, i64> Step(i64 index) {{ return Result::Ok(index); }} \
             i64 Main() {{ mut i64 total = 0_i64; mut i64 index = 0_i64; \
             while (index < 8_i64) {{ index = index + 1_i64; \
             i64 value = match Step(index) {{ \
             Result::Ok(value) => value, \
             Result::Error(code) => {{ if (code > 0_i64) {{ break; }} continue; }}, }}; \
             total = total + value; }} \
             return total; }}"
        ),
    );
}

/// `when` guards: a guarded arm falls through to the next arm when its guard is false, both as
/// a value and as a statement.
#[test]
fn guarded_arms_lower_as_conditional_arm_selection() {
    let functions = lower_program(
        "guarded arms",
        "enum Choice { Value(i64 value), Empty } \
         i64 Score(Choice choice) { return match choice { \
         Choice::Value(v) when v > 10_i64 => 2_i64, \
         Choice::Value(v) when v > 0_i64 => 1_i64, \
         Choice::Value(_) => 0_i64, \
         Choice::Empty => -1_i64, }; } \
         i64 Main(Choice choice) { mut i64 total = 0_i64; \
         match choice { Choice::Value(v) when v > 0_i64 => { total = v; }, _ => { total = -1_i64; }, }; \
         return total + Score(choice); }",
    );
    let score = clif_of(&functions, "Score");
    assert!(score.matches("icmp").count() >= 4, "each guard and tag test compares explicitly: {score}");
}

/// A bare-name guard (`when strict =>`) is the guard expression, not a lambda over the arm body,
/// both as a value arm and as a statement arm.
#[test]
fn bare_name_guards_lower_as_conditional_arm_selection() {
    let functions = lower_program(
        "bare-name guards",
        "enum Choice { Value(i64 value), Empty } \
         i64 Score(Choice choice, bool strict) { return match choice { \
         Choice::Value(v) when strict => v, \
         Choice::Value(_) => 0_i64, \
         Choice::Empty => -1_i64, }; } \
         i64 Main(Choice choice, bool strict) { mut i64 total = 0_i64; \
         match choice { Choice::Value(v) when (strict) => { total = v; }, _ => { total = -1_i64; }, }; \
         return total + Score(choice, strict); }",
    );
    let score = clif_of(&functions, "Score");
    assert!(score.contains("brif"), "the bare-name guard selects between arms: {score}");
}

/// A whole-scrutinee binding arm of an enum match binds the enum value itself.
#[test]
fn whole_scrutinee_binding_arm_of_an_enum_match_lowers() {
    lower_program(
        "whole-scrutinee binding",
        "enum Choice { Value(i64 value), Empty } \
         i64 Weight(Choice choice) { return match choice { Choice::Value(v) => v, Choice::Empty => 0_i64, }; } \
         i64 Main(Choice choice) { return match choice { Choice::Empty => -1_i64, other => Weight(other), }; }",
    );
}

/// By-value matches over primitive scrutinees: integer (unsuffixed and suffixed literal
/// patterns, whole-value binding), `bool`, `char`, and `string` (content comparison).
#[test]
fn primitive_scrutinee_matches_lower_by_value() {
    let functions = lower_program(
        "primitive scrutinees",
        "i64 Integer(i64 count) { return match count { 0 => 10_i64, 7_i64 => 20_i64, other => other, }; } \
         i64 Flag(bool flag) { return match flag { true => 1_i64, false => 0_i64, }; } \
         i64 Letter(char letter) { return match letter { 'a' => 1_i64, _ => 0_i64, }; } \
         i64 Text(string name) { return match name { \"alpha\" => 1_i64, \"beta\" => 2_i64, _ => 0_i64, }; } \
         i64 Guarded(i64 count) { return match count { small when small < 10_i64 => 1_i64, _ => 0_i64, }; }",
    );
    let integer = clif_of(&functions, "Integer");
    assert!(integer.contains("iconst.i64 0") && integer.contains("iconst.i64 7"), "{integer}");
    let text = clif_of(&functions, "Text");
    // Short literal patterns compare length and bytes in place; no literal is materialized.
    assert!(
        text.matches("bxor").count() >= 2 && !text.contains("%str_new") && !text.contains("%str_eq"),
        "string patterns compare content in place: {text}"
    );
}

/// A primitive statement match need not be exhaustive: no applicable arm continues after it.
#[test]
fn non_exhaustive_primitive_statement_match_continues_after_the_match() {
    lower_program(
        "non-exhaustive statement match",
        "i64 Main(i64 count) { mut i64 total = 0_i64; match count { 1 => { total = 1_i64; }, 2 => { total = 2_i64; }, }; return total; }",
    );
}

/// Corelib `Process.Capture`: a value match over `Result<unit, ProcessError>` that re-wraps the
/// error into a different `Result` instantiation and binds the zero-sized `unit` payload.
#[test]
fn value_match_rewrapping_an_error_and_binding_a_unit_payload_lowers() {
    lower_program(
        "Capture re-wrap with unit binding",
        &format!(
            "{OPTION_RESULT}enum ProcessError {{ Closed, ExitCode(i64 code, string stderr) }} \
             type ProcessOutput {{ i64 exitCode }} \
             Result<unit, ProcessError> Close(i64 token) {{ if token > 0_i64 {{ return Result::Error(ProcessError::Closed); }} return Result::Ok(()); }} \
             Result<ProcessOutput, ProcessError> CapturedOutput(i64 exit) {{ return Result::Ok(ProcessOutput {{ exitCode: exit }}); }} \
             Result<ProcessOutput, ProcessError> Capture(i64 token) {{ \
             Result<unit, ProcessError> closed = Close(token); \
             return match closed {{ \
             Result::Error(error) => Result::Error(error), \
             Result::Ok(value) => CapturedOutput(token), }}; }}"
        ),
    );
}

/// ProcessSuite `TryWrite` loop shapes exactly: the nested `Option<i64>` match as the arm
/// expression and inside a block arm, with unit block arms including an empty `{ }`.
#[test]
fn nested_option_match_as_arm_expression_and_inside_a_block_arm_lowers() {
    lower_program(
        "ProcessSuite nested Option<i64> arms",
        &format!(
            "{OPTION_RESULT}enum IoError {{ Closed }} \
             Result<Option<i64>, IoError> TryWrite(i64 count) {{ return Result::Ok(Option::Some(count)); }} \
             unit Fail() {{ return; }} \
             i64 Main() {{ mut i64 written = 0_i64; mut bool progressed = false; mut bool eof = false; \
             while (written < 10_i64) {{ \
             match TryWrite(written) {{ \
             Result::Error(_) => Fail(), \
             Result::Ok(sent) => match sent {{ \
             Option::Some(bytes) => {{ written = written + bytes + 1_i64; progressed = bytes > 0_i64; }}, \
             Option::None => {{ }}, }}, }}; \
             match TryWrite(written) {{ \
             Result::Error(_) => Fail(), \
             Result::Ok(read) => {{ \
             match read {{ Option::Some(bytes) => {{ if (bytes == 0_i64) {{ eof = true; }} }}, Option::None => {{ }}, }}; \
             }}, }}; \
             }} \
             return written; }}"
        ),
    );
}

/// `check` and ISLE agree on refutable payload patterns: the legality gate rejects the match with
/// E1304 naming the uncovered value, before lowering, instead of an internal error.
#[test]
fn refutable_payload_literal_without_a_catch_all_is_rejected_by_the_gate() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result { Ok(i64 value), Error(i64 error) } \
         i64 Main(Result r) { return match r { Result::Ok(7_i64) => 1_i64, Result::Error(_) => 0_i64, }; }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root)
        .into_iter()
        .map(|key| SyntaxModuleItem { key, symbol: item_name(db, key).unwrap().unwrap().to_string() })
        .collect::<Vec<_>>();
    let error = lower_syntax_program(&input, isa.as_ref(), &items).expect_err("the gate rejects the match");
    let rendered = error.to_string();
    assert!(rendered.contains("E1304") && rendered.contains("Result::Ok(_)"), "{rendered}");
    assert!(!rendered.contains("E2102") && !rendered.contains("NonExhaustiveMatch"), "{rendered}");
}

/// A `unit` payload binding read as a value is the unit constant: re-wrapped into another enum
/// instantiation and returned from a `unit` function.
#[test]
fn unit_payload_binding_read_as_a_value_lowers() {
    lower_program(
        "unit binding as a value",
        &format!(
            "{OPTION_RESULT}enum IoError {{ Closed }} \
             Result<unit, IoError> Close(i64 token) {{ if token > 0_i64 {{ return Result::Error(IoError::Closed); }} return Result::Ok(()); }} \
             Result<unit, i64> Rewrap(i64 token) {{ return match Close(token) {{ \
             Result::Ok(value) => Result::Ok(value), Result::Error(_) => Result::Error(token), }}; }} \
             unit Main(i64 token) {{ match Close(token) {{ \
             Result::Ok(value) => {{ return value; }}, Result::Error(_) => {{ }}, }}; return; }}"
        ),
    );
}

/// A generic record whose field is a generic enum over the record's own parameter, instantiated
/// for a scalar and a managed record `T`. Every match below reads that field inside a
/// specialization (a method of `Holder<T>` or a generic function), so its enum match fact must be
/// proven in the specialization's environment.
const GENERIC_FIELD_HOLDER: &str = "enum Option<T> { Some(T value), None } \
     type Point { i64 x } \
     type Holder<T> { Option<T[]> slot, Option<T> one } \
     type Outer<T> { Holder<T> inner } ";

/// Instantiations of the probed generic items for `i64` and `Point`.
const GENERIC_FIELD_MAIN: &str = "Holder<i64> Ints() { i64[] values = [3_i64, 4_i64]; \
     return Holder<i64> { slot: Option::Some(values), one: Option::None }; } \
     Holder<Point> Points() { Point[] values = [Point { x: 5_i64 }]; \
     return Holder<Point> { slot: Option::Some(values), one: Option::Some(Point { x: 6_i64 }) }; } ";

/// Lower every method and function of `source`, generic specializations included.
fn lower_generic_program(label: &str, source: &str) {
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let mut keys = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::MethodDefinition);
    keys.extend(find_function_definitions(db, root));
    let items = keys
        .into_iter()
        .map(|key| SyntaxModuleItem { key, symbol: item_name(db, key).unwrap().unwrap().to_string() })
        .collect::<Vec<_>>();
    if let Err(error) = lower_syntax_program(&input, isa.as_ref(), &items) {
        panic!("{label} must lower: {error}");
    }
}

/// One probed form: generic declarations (`Holder<T>` methods are spliced in through `methods`)
/// and a `Main` that calls them for `i64` and `Point`.
fn lower_generic_field_form(label: &str, methods: &str, functions: &str, main: &str) {
    let source = GENERIC_FIELD_HOLDER.replace(
        "type Holder<T> { Option<T[]> slot, Option<T> one }",
        &if methods.is_empty() {
            "type Holder<T> { Option<T[]> slot, Option<T> one }".to_string()
        } else {
            format!("type Holder<T> {{ Option<T[]> slot, Option<T> one, {methods} }}")
        },
    );
    lower_generic_program(label, &format!("{source}{GENERIC_FIELD_MAIN}{functions} i64 Main() {{ {main} }}"));
}

#[test]
fn wildcard_payload_match_on_this_field_in_a_generic_method_lowers() {
    lower_generic_field_form(
        "wildcard on this.slot",
        "i64 Has() { return match this.slot { Option::Some(_) => 1_i64, Option::None => 0_i64, }; }",
        "",
        "return Ints().Has() + Points().Has();",
    );
}

#[test]
fn wildcard_payload_match_on_a_bare_field_in_a_generic_method_lowers() {
    lower_generic_field_form(
        "wildcard on bare slot",
        "i64 Has() { return match slot { Option::Some(_) => 1_i64, Option::None => 0_i64, }; }",
        "",
        "return Ints().Has() + Points().Has();",
    );
}

#[test]
fn wildcard_payload_match_on_a_parameter_field_in_a_generic_function_lowers() {
    lower_generic_field_form(
        "wildcard on h.slot and (h).slot",
        "",
        "i64 Has<T>(Holder<T> h) { return match h.slot { Option::Some(_) => 1_i64, Option::None => 0_i64, }; } \
         i64 HasGrouped<T>(Holder<T> h) { return match (h).slot { Option::Some(_) => 1_i64, Option::None => 0_i64, }; } \
         i64 HasOne<T>(Holder<T> h) { return match h.one { Option::Some(_) => 1_i64, Option::None => 0_i64, }; }",
        "return Has<i64>(Ints()) + Has<Point>(Points()) + HasGrouped<i64>(Ints()) + HasGrouped<Point>(Points()) \
         + HasOne<i64>(Ints()) + HasOne<Point>(Points());",
    );
}

#[test]
fn let_bound_value_match_on_a_parameter_field_in_a_generic_function_lowers() {
    lower_generic_field_form(
        "let x = match h.slot",
        "",
        "T FirstOr<T>(Holder<T> h, T fallback) { let x = match h.slot { Option::Some(values) => values[0], Option::None => fallback, }; return x; } \
         i64 Count<T>(Holder<T> h) { let n = match h.slot { Option::Some(values) => 2_i64, Option::None => 0_i64, }; return n; } \
         T Typed<T>(Holder<T> h, T fallback) { T x = match (h).slot { Option::Some(values) => values[0], Option::None => fallback, }; return x; }",
        "Point p = FirstOr<Point>(Points(), Point { x: 0_i64 }); Point q = Typed<Point>(Points(), Point { x: 0_i64 }); \
         return FirstOr<i64>(Ints(), 0_i64) + Count<i64>(Ints()) + Count<Point>(Points()) + Typed<i64>(Ints(), 0_i64) + p.x + q.x;",
    );
}

#[test]
fn let_bound_value_match_on_this_field_in_a_generic_method_lowers() {
    lower_generic_field_form(
        "let x = match this.slot",
        "T FirstOr(T fallback) { let x = match this.slot { Option::Some(values) => values[0], Option::None => fallback, }; return x; } \
         T OneOr(T fallback) { let x = match one { Option::Some(value) => value, Option::None => fallback, }; return x; }",
        "",
        "Point p = Points().FirstOr(Point { x: 0_i64 }); Point q = Points().OneOr(Point { x: 0_i64 }); \
         return Ints().FirstOr(0_i64) + Ints().OneOr(1_i64) + p.x + q.x;",
    );
}

#[test]
fn match_on_a_nested_generic_record_field_lowers() {
    lower_generic_field_form(
        "match o.inner.slot",
        "",
        "i64 Has<T>(Outer<T> o) { return match o.inner.slot { Option::Some(_) => 1_i64, Option::None => 0_i64, }; } \
         T FirstOr<T>(Outer<T> o, T fallback) { let x = match o.inner.slot { Option::Some(values) => values[0], Option::None => fallback, }; return x; }",
        "Outer<i64> ints = Outer<i64> { inner: Ints() }; Outer<Point> points = Outer<Point> { inner: Points() }; \
         Point p = FirstOr<Point>(points, Point { x: 0_i64 }); \
         return Has<i64>(ints) + Has<Point>(points) + FirstOr<i64>(ints, 0_i64) + p.x;",
    );
}

#[test]
fn statement_and_guarded_matches_on_a_generic_record_field_lower() {
    lower_generic_field_form(
        "statement and guarded match on h.slot",
        "i64 Mark(i64 limit) { mut i64 total = 0_i64; \
         match this.slot { Option::Some(_) when limit > 0_i64 => { total = 2_i64; }, Option::Some(values) => { total = 1_i64; }, Option::None => { }, }; \
         return total; }",
        "i64 Walk<T>(Holder<T> h, i64 limit) { mut i64 total = 0_i64; \
         match h.slot { Option::Some(values) when limit > 0_i64 => { total = 2_i64; }, Option::Some(_) => { total = 1_i64; }, Option::None => { }, }; \
         i64 extra = match h.slot { Option::Some(_) when limit > 0_i64 => 4_i64, _ => 0_i64, }; return total + extra; }",
        "return Ints().Mark(1_i64) + Points().Mark(0_i64) + Walk<i64>(Ints(), 1_i64) + Walk<Point>(Points(), 0_i64);",
    );
}

#[test]
fn return_match_with_a_named_binding_on_a_generic_record_field_lowers() {
    lower_generic_field_form(
        "return match h.slot with a binding",
        "T FirstOr(T fallback) { return match this.slot { Option::Some(values) => values[0], Option::None => fallback, }; }",
        "T First<T>(Holder<T> h, T fallback) { return match h.slot { Option::Some(values) => values[0], Option::None => fallback, }; }",
        "Point p = First<Point>(Points(), Point { x: 0_i64 }); Point q = Points().FirstOr(Point { x: 0_i64 }); \
         return First<i64>(Ints(), 0_i64) + Ints().FirstOr(0_i64) + p.x + q.x;",
    );
}

/// Indexes of the `call` instructions in `clif` whose callee is declared as `%symbol`, or as a
/// program function `%symbol#...`.
fn calls_to(clif: &str, symbol: &str) -> Vec<usize> {
    let callees = clif
        .lines()
        .filter_map(|line| {
            let (name, declared) = line.trim_start().split_once(" = %")?;
            let declared = declared.split(' ').next()?;
            (declared.split('#').next() == Some(symbol)).then(|| name.to_owned())
        })
        .collect::<Vec<_>>();
    clif.lines()
        .enumerate()
        .filter(|(_, line)| {
            line.split("call ")
                .nth(1)
                .and_then(|rest| rest.split('(').next())
                .is_some_and(|callee| callees.iter().any(|name| name == callee))
        })
        .map(|(index, _)| index)
        .collect()
}

/// A guard runs code that can collect before later arms test the scrutinee, and a string
/// pattern longer than the inline compare limit is materialized before its test: a scrutinee
/// that only the match references is then rooted from before the first test to the end of the
/// match. Before the fix a collection in `Busy` freed the `Make` result, and the next arm tested
/// whatever reused its storage. A match whose tests cannot collect adds no root.
#[test]
fn match_scrutinee_is_rooted_while_its_tests_can_collect() {
    let long = "x".repeat(65);
    let functions = lower_program(
        "scrutinee rooting",
        &format!(
            "enum Choice {{ Value(i64 value), Empty }} \
             Choice Make(i64 n) {{ return Choice::Value(n); }} \
             string Fresh(i64 n) {{ return \"k${{n}}\"; }} \
             bool Busy(i64 n) {{ Choice other = Make(n); return n > 3_i64; }} \
             i64 GuardedScrutinee(i64 n) {{ return match Make(n) {{ \
             Choice::Value(_) when Busy(n) => 2_i64, Choice::Value(v) => v, Choice::Empty => 0_i64, }}; }} \
             i64 PlainScrutinee(i64 n) {{ return match Make(n) {{ Choice::Value(v) => v, Choice::Empty => 0_i64, }}; }} \
             i64 LongPatternScrutinee(i64 n) {{ return match Fresh(n) {{ \"{long}\" => 1_i64, _ => 0_i64, }}; }} \
             i64 ShortPatternScrutinee(i64 n) {{ return match Fresh(n) {{ \"k1\" => 1_i64, _ => 0_i64, }}; }}"
        ),
    );

    let guarded = clif_of(&functions, "GuardedScrutinee");
    let guard_call = *calls_to(guarded, "Busy")
        .first()
        .unwrap_or_else(|| panic!("the guard calls Busy: {guarded}"));
    let make_call = *calls_to(guarded, "Make").first().unwrap_or_else(|| panic!("the scrutinee calls Make: {guarded}"));
    assert!(
        calls_to(guarded, "gc_register_root").iter().any(|register| (make_call..guard_call).contains(register)),
        "the scrutinee is rooted before the guard runs: {guarded}"
    );
    assert!(
        calls_to(guarded, "gc_unregister_root").iter().any(|release| *release > guard_call),
        "the scrutinee root is released after the guard: {guarded}"
    );

    let long_pattern = clif_of(&functions, "LongPatternScrutinee");
    let literal = *calls_to(long_pattern, "str_new")
        .first()
        .unwrap_or_else(|| panic!("the long pattern is materialized: {long_pattern}"));
    let fresh_call = *calls_to(long_pattern, "Fresh").first().unwrap_or_else(|| panic!("{long_pattern}"));
    assert!(
        calls_to(long_pattern, "gc_register_root").iter().any(|register| (fresh_call..literal).contains(register)),
        "the scrutinee is rooted before the pattern allocates: {long_pattern}"
    );

    for name in ["PlainScrutinee", "ShortPatternScrutinee"] {
        let clif = clif_of(&functions, name);
        assert!(calls_to(clif, "gc_register_root").is_empty(), "{name}: no test can collect, no root: {clif}");
    }
}
