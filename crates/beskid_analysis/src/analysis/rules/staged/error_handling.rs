use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::syntax::Program;
use crate::resolve::Resolution;
use crate::syntax::Spanned;
use crate::types::try_desugar::invalid_try_expression_spans;

impl SemanticPipelineRule {
    pub(super) fn stage7_error_handling(
        &self,
        ctx: &mut RuleContext,
        program: &Spanned<Program>,
        resolution: &Resolution,
    ) {
        for span in invalid_try_expression_spans(resolution, program) {
            ctx.emit_issue(span, SemanticIssueKind::TypeInvalidTryTarget);
        }
    }
}
