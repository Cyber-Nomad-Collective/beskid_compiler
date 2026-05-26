use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::hir::HirProgram;
use crate::resolve::Resolution;
use crate::syntax::Spanned;
use crate::types::context::try_infer::invalid_try_expression_spans;

impl SemanticPipelineRule {
    pub(super) fn stage7_error_handling(
        &self,
        ctx: &mut RuleContext,
        hir: &Spanned<HirProgram>,
        resolution: &Resolution,
    ) {
        for span in invalid_try_expression_spans(resolution, hir) {
            ctx.emit_issue(span, SemanticIssueKind::TypeInvalidTryTarget);
        }
    }
}
