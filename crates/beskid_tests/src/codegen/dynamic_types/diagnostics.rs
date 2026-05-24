use beskid_analysis::syntax::SpanInfo;
use beskid_codegen::diagnostics::codegen_error_to_diagnostic;
use beskid_codegen::errors::CodegenError;

#[test]
fn dynamic_ineligible_mapping_maps_to_e2013() {
    let span = SpanInfo {
        start: 0,
        end: 1,
        line_col_start: (1, 1),
        line_col_end: (1, 2),
    };
    let error = CodegenError::IneligibleSerializeMapping {
        span,
        src_name: "Source".to_string(),
        dst_name: "Target".to_string(),
    };
    let diagnostic = codegen_error_to_diagnostic("test.bd", "x", &error);

    assert_eq!(diagnostic.code.as_deref(), Some("E2013"));
    assert!(diagnostic.message.contains("Source"));
    assert!(diagnostic.message.contains("Target"));
    assert!(diagnostic.message.contains("Serialization Mod"));
}
