use beskid_analysis::Severity;
use beskid_analysis::analysis::SemanticIssueKind;
use beskid_analysis::syntax::SpanInfo;

fn span() -> SpanInfo {
    SpanInfo {
        start: 1,
        end: 2,
        line_col_start: (3, 4),
        line_col_end: (3, 5),
    }
}

#[test]
fn resolve_private_item_issue_contract_is_stable() {
    let issue = SemanticIssueKind::ResolvePrivateItemInModule {
        module_path: "foo.bar".to_string(),
        name: "baz".to_string(),
    };
    assert_eq!(issue.code(), "E1107");
    assert_eq!(issue.severity(), Severity::Error);
    assert!(issue.message().contains("private item `baz`"));
    assert!(issue.help().is_some());
}

#[test]
fn invalid_event_subscription_target_issue_contract_is_stable() {
    let issue = SemanticIssueKind::TypeInvalidEventSubscriptionTarget;
    assert_eq!(issue.code(), "E1221");
    assert_eq!(issue.severity(), Severity::Error);
    assert_eq!(issue.label(), "invalid event subscription target");
    assert!(
        issue
            .message()
            .contains("event subscription target must be an event field")
    );
    assert!(issue.help().is_some());
}

#[test]
fn invalid_try_target_issue_contract_is_stable() {
    let issue = SemanticIssueKind::TypeInvalidTryTarget;
    assert_eq!(issue.code(), "E1222");
    assert_eq!(issue.severity(), Severity::Error);
    assert_eq!(issue.label(), "invalid try target");
    assert!(
        issue
            .message()
            .contains("try operator requires a Result value with an Ok payload")
    );
    assert!(issue.help().is_some());
}

#[test]
fn invalid_conformance_target_issue_contract_is_stable() {
    let issue = SemanticIssueKind::ResolveInvalidConformanceTarget {
        name: "NotContract".to_string(),
    };
    assert_eq!(issue.code(), "E1607");
    assert_eq!(issue.severity(), Severity::Error);
    assert_eq!(issue.label(), "invalid conformance target");
    assert!(issue.message().contains("`NotContract` is not a contract"));
    assert!(issue.help().is_some());
}

#[test]
fn attribute_target_mismatch_issue_contract_is_stable() {
    let issue = SemanticIssueKind::AttributeTargetNotAllowed {
        attribute: "Extern".to_string(),
        target: "ModuleDeclaration".to_string(),
        allowed: vec!["ContractDeclaration".to_string()],
    };

    assert_eq!(issue.code(), "E1809");
    assert_eq!(issue.severity(), Severity::Error);
    assert_eq!(issue.label(), "attribute target not allowed");
    assert!(
        issue
            .message()
            .contains("attribute `Extern` cannot be applied to `ModuleDeclaration`")
    );
    assert_eq!(
        issue.help().as_deref(),
        Some("allowed targets: ContractDeclaration")
    );
}

#[test]
fn duplicate_attribute_target_issue_contract_is_stable() {
    let issue = SemanticIssueKind::DuplicateAttributeDeclarationTarget {
        target: "TypeDeclaration".to_string(),
        previous: span(),
    };

    assert_eq!(issue.code(), "E1806");
    assert_eq!(issue.severity(), Severity::Error);
    assert!(
        issue
            .message()
            .contains("duplicate target `TypeDeclaration`")
    );
    assert_eq!(
        issue.help().as_deref(),
        Some("target already listed at line 3, column 4")
    );
}

#[test]
fn duplicate_definition_uses_previous_span_help() {
    let issue = SemanticIssueKind::DuplicateDefinitionName {
        name: "User".to_string(),
        previous: span(),
    };
    assert_eq!(issue.code(), "E1001");
    assert_eq!(issue.severity(), Severity::Error);
    assert!(issue.message().contains("duplicate definition name `User`"));
    assert_eq!(
        issue.help().as_deref(),
        Some("previously defined at line 3, column 4")
    );
}

#[test]
fn warning_issue_contract_is_stable() {
    let issue = SemanticIssueKind::TypeImplicitNumericCast {
        from: "i64".to_string(),
        to: "i32".to_string(),
    };
    assert_eq!(issue.code(), "W1203");
    assert_eq!(issue.severity(), Severity::Warning);
    assert_eq!(issue.label(), "implicit numeric cast");
    assert!(
        issue
            .message()
            .contains("implicit numeric cast from i64 to i32")
    );
    assert!(issue.help().is_some());
}

#[test]
fn file_scoped_module_not_first_issue_contract_is_stable() {
    let issue = SemanticIssueKind::FileScopedModuleNotFirstItem {
        module_path: "app.core".to_string(),
    };
    assert_eq!(issue.code(), "E1505");
    assert_eq!(issue.severity(), Severity::Error);
    assert_eq!(issue.label(), "file-scoped module must be first item");
    assert!(issue.message().contains("must be the first top-level item"));
    assert!(issue.help().is_some());
}

#[test]
fn duplicate_file_scoped_module_issue_contract_is_stable() {
    let issue = SemanticIssueKind::DuplicateFileScopedModule {
        module_path: "app.other".to_string(),
    };
    assert_eq!(issue.code(), "E1506");
    assert_eq!(issue.severity(), Severity::Error);
    assert_eq!(issue.label(), "duplicate file-scoped module");
    assert!(issue.message().contains("duplicate file-scoped module declaration"));
    assert!(issue.help().is_some());
}

#[test]
fn forbidden_module_declaration_in_file_scoped_issue_contract_is_stable() {
    let issue = SemanticIssueKind::ModuleDeclarationForbiddenInFileScopedModule;
    assert_eq!(issue.code(), "E1507");
    assert_eq!(issue.severity(), Severity::Error);
    assert_eq!(issue.label(), "module declaration not allowed");
    assert!(issue.message().contains("not allowed in a file-scoped module file"));
    assert!(issue.help().is_some());
}

