use crate::analysis::rules::{Rule, RuleContext};
use crate::syntax::{Program, SpanInfo, Spanned};
use beskid_pipeline::{PipelineObserver, observe_phase, phases};
use std::collections::HashSet;

mod control_flow;
mod definitions;
mod missing_imports;
mod name_resolution;
mod naming_style;
mod type_checking;
mod visibility;

pub struct SemanticPipelineRule;

impl Rule for SemanticPipelineRule {
    fn name(&self) -> &'static str {
        "semantic_pipeline"
    }

    fn run(&self, ctx: &mut RuleContext, program: &Program) {
        self.run_stages(ctx, program, None);
    }
}

impl SemanticPipelineRule {
    pub(crate) fn run_stages(&self, ctx: &mut RuleContext, program: &Program, pipeline: Option<&dyn PipelineObserver>) {
        let span = program.items.first().map(|item| item.span).unwrap_or(SpanInfo {
            start: 0,
            end: 0,
            line_col_start: (1, 1),
            line_col_end: (1, 1),
        });
        let program = Spanned::new(program.clone(), span);

        observe_stage(pipeline, phases::SEMANTIC_DEFINITIONS, || {
            self.stage0_collect_definitions(ctx, &program);
        });
        let mut unqualified_variant_calls = Vec::new();
        observe_stage(pipeline, phases::SEMANTIC_CONTROL_FLOW, || {
            unqualified_variant_calls = self.stage3_control_flow_and_patterns(ctx, &program);
        });

        let mut unresolved_value_spans = HashSet::new();
        let resolution = observe_stage_optional(pipeline, phases::SEMANTIC_NAME_RESOLUTION, || {
            self.stage1_name_resolution(ctx, &program, &mut unresolved_value_spans)
        });
        self.emit_unqualified_enum_constructors(ctx, unqualified_variant_calls, &unresolved_value_spans);
        let Some(resolution) = resolution else {
            return;
        };

        observe_stage(pipeline, "semantic.missing_imports", || {
            self.stage_missing_imports(ctx, &program, &resolution);
        });

        observe_stage(pipeline, phases::SEMANTIC_VISIBILITY, || {
            self.stage5_modules_and_visibility(ctx, &program, &resolution);
        });
        // Typing, contract conformance and `?` legality are the query gate's obligations
        // (`beskid_queries::semantic_diagnostics_for_roots`); this pipeline keeps the structural
        // and style rules only (`stage2_type_check` below is structural-immutability-only).
        observe_stage(pipeline, phases::SEMANTIC_TYPE_CHECK, || {
            self.stage2_type_check(ctx, &program);
        });
        observe_stage(pipeline, phases::SEMANTIC_NAMING_STYLE, || {
            self.stage_naming_style(ctx, &program.node);
        });
    }
}

fn observe_stage<O: PipelineObserver + ?Sized>(obs: Option<&O>, id: &'static str, f: impl FnOnce()) {
    if let Some(o) = obs {
        observe_phase(Some(o), id, f);
    } else {
        f();
    }
}

fn observe_stage_optional<T, O: PipelineObserver + ?Sized>(
    obs: Option<&O>,
    id: &'static str,
    f: impl FnOnce() -> Option<T>,
) -> Option<T> {
    if let Some(o) = obs {
        let mut result = None;
        observe_phase(Some(o), id, || {
            result = f();
        });
        result
    } else {
        f()
    }
}
