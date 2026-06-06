use crate::codegen::util::lower_resolve_type;
use beskid_codegen::{DYNAMIC_TYPE_NAME, lower_program};

#[test]
fn dynamic_type_alias_lowers_without_codegen_failure() {
    let source =
        format!("type {DYNAMIC_TYPE_NAME} {{ i64 payload }} i64 main() {{ i64 x = 1; return x; }}");
    let (hir, resolution, typed) = lower_resolve_type(&source);
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("dynamic alias program should codegen");
    assert_eq!(artifact.functions.len(), 1);
    let clif = artifact.functions[0].function.to_string();
    assert!(
        clif.contains("return"),
        "expected lowered main with return: {clif}"
    );
}
