use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::diagnostics::Severity;
use crate::analysis::rules::RuleContext;
use crate::types::{TypeError, TypeResult, format_type_id};

pub(crate) fn emit_type_error(
    ctx: &mut RuleContext,
    error: TypeError,
    result: Option<&TypeResult>,
) {
    match error {
        TypeError::UnknownType { span, name } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeUnknownType { name });
        }
        TypeError::UnknownValueType { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeUnknownValueType);
        }
        TypeError::UnknownStructType { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeUnknownStructType);
        }
        TypeError::InvalidMemberTarget { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeInvalidMemberTarget);
        }
        TypeError::UnknownEnumType { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeUnknownEnumType);
        }
        TypeError::UnknownStructField { span, name } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeUnknownStructField { name });
        }
        TypeError::UnknownEnumVariant { span, name } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeUnknownEnumVariant { name });
        }
        TypeError::MissingStructField { span, name } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeMissingStructField { name });
        }
        TypeError::MissingTypeAnnotation { span, name } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeMissingTypeAnnotation { name });
        }
        TypeError::MissingTypeArguments { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeMissingTypeArguments);
        }
        TypeError::GenericArgumentMismatch {
            span,
            expected,
            actual,
        } => {
            ctx.emit_issue(
                span,
                SemanticIssueKind::TypeGenericArgumentMismatch { expected, actual },
            );
        }
        TypeError::TypeMismatch {
            span,
            expected,
            actual,
        } => {
            let expected_name = render_type(result, expected);
            let actual_name = render_type(result, actual);
            ctx.emit_issue(
                span,
                SemanticIssueKind::TypeMismatch {
                    expected_name,
                    actual_name,
                },
            );
        }
        TypeError::MatchArmTypeMismatch {
            span,
            expected,
            actual,
        } => {
            let expected_name = render_type(result, expected);
            let actual_name = render_type(result, actual);
            ctx.emit_issue(
                span,
                SemanticIssueKind::TypeMatchArmMismatch {
                    expected_name,
                    actual_name,
                },
            );
        }
        TypeError::CallArityMismatch {
            span,
            expected,
            actual,
        } => {
            ctx.emit_issue(
                span,
                SemanticIssueKind::TypeCallArityMismatch { expected, actual },
            );
        }
        TypeError::CallArgumentMismatch {
            span,
            expected,
            actual,
        } => {
            let expected_name = render_type(result, expected);
            let actual_name = render_type(result, actual);
            ctx.emit_issue(
                span,
                SemanticIssueKind::TypeCallArgumentMismatch {
                    expected_name,
                    actual_name,
                },
            );
        }
        TypeError::EnumConstructorMismatch {
            span,
            expected,
            actual,
        } => {
            ctx.emit_issue(
                span,
                SemanticIssueKind::TypeEnumConstructorMismatch { expected, actual },
            );
        }
        TypeError::UnknownCallTarget { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeUnknownCallTarget);
        }
        TypeError::InvalidBinaryOp { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeInvalidBinaryOp);
        }
        TypeError::InvalidUnaryOp { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeInvalidUnaryOp);
        }
        TypeError::NonBoolCondition { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeNonBoolCondition);
        }
        TypeError::UnsupportedExpression { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeUnsupportedExpression);
        }
        TypeError::InvalidTryTarget { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeInvalidTryTarget);
        }
        TypeError::InvalidEventInvocationScope { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeInvalidEventInvocationScope);
        }
        TypeError::InvalidEventCapacity { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeInvalidEventCapacity);
        }
        TypeError::InvalidEventSubscriptionTarget { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeInvalidEventSubscriptionTarget);
        }
        TypeError::SpawnTargetNotFiberCompatible { span } => {
            ctx.emit_issue(span, SemanticIssueKind::SpawnTargetNotFiberCompatible);
        }
        TypeError::JoinWouldDeadlock { span } => {
            ctx.emit_issue(span, SemanticIssueKind::JoinWouldDeadlock);
        }
        TypeError::StackReferenceEscapesSpawn { span } => {
            ctx.emit_issue(span, SemanticIssueKind::StackReferenceEscapesSpawn);
        }
        TypeError::ReturnTypeMismatch {
            span,
            expected,
            actual,
        } => {
            let expected_name = render_type(result, expected);
            let actual_name = actual
                .map(|type_id| render_type(result, type_id))
                .unwrap_or_else(|| "unit".to_string());
            ctx.emit_issue(
                span,
                SemanticIssueKind::TypeReturnMismatch {
                    expected_name,
                    actual_name,
                },
            );
        }
        TypeError::NonIterableForTarget { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeNonIterableForTarget);
        }
        TypeError::IterableNextArityMismatch {
            span,
            expected,
            actual,
        } => {
            ctx.emit_issue(
                span,
                SemanticIssueKind::TypeIterableNextArityMismatch { expected, actual },
            );
        }
        TypeError::IterableNextReturnNotOption { span } => {
            ctx.emit_issue(span, SemanticIssueKind::TypeIterableNextReturnNotOption);
        }
        TypeError::IterableOptionSomeArityMismatch {
            span,
            expected,
            actual,
        } => {
            ctx.emit_issue(
                span,
                SemanticIssueKind::TypeIterableOptionSomeArityMismatch { expected, actual },
            );
        }
        TypeError::ExternInvalidAbi { span, .. } => {
            ctx.emit_simple(
                span,
                "T0901",
                "invalid extern ABI; only Abi:\"C\" is supported",
                "extern attribute",
                Some("Use [Extern(Abi:\"C\", Library:\"...\")] on the contract".to_string()),
                Severity::Error,
            );
        }
        TypeError::ExternMissingLibrary { span } => {
            ctx.emit_simple(
                span,
                "T0902",
                "extern contract missing Library",
                "extern attribute",
                Some("Provide Library:\"<soname>\"; e.g., libc.so.6 on Linux".to_string()),
                Severity::Error,
            );
        }
        TypeError::ExternDisallowedParamType { span, method } => {
            ctx.emit_simple(
                span,
                "T0903",
                format!("extern param type not allowed in `{}`", method),
                "parameter type",
                Some("Allowed: bool, u8, i32, i64, f64; for raw pointers, pass as i64 (pointer-sized) for now".to_string()),
                Severity::Error,
            );
        }
        TypeError::ExternDisallowedReturnType { span, method } => {
            ctx.emit_simple(
                span,
                "T0904",
                format!("extern return type not allowed in `{}`", method),
                "return type",
                Some("Allowed: bool, u8, i32, i64, f64, or unit (no value)".to_string()),
                Severity::Error,
            );
        }
    }
}

pub(crate) fn emit_cast_intent_warnings(ctx: &mut RuleContext, result: &TypeResult) {
    let entry_source_path = ctx.options.entry_source_path.as_ref();
    let intents: Vec<_> = result.cast_intents_for_entry(entry_source_path).collect();
    for intent in intents {
        let from = render_type_from_result(result, intent.from);
        let to = render_type_from_result(result, intent.to);
        ctx.emit_issue(
            intent.span,
            SemanticIssueKind::TypeImplicitNumericCast { from, to },
        );
    }
}

fn render_type(result: Option<&TypeResult>, type_id: crate::types::TypeId) -> String {
    let Some(result) = result else {
        return format!("type#{}", type_id.0);
    };
    format_type_id(result, None, type_id)
}

fn render_type_from_result(result: &TypeResult, type_id: crate::types::TypeId) -> String {
    format_type_id(result, None, type_id)
}
