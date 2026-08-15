use crate::analysis::rules::{Rule, RuleContext};
use crate::syntax::{Program, SpanInfo, Spanned};
use beskid_pipeline::{observe_phase, phases, PipelineObserver};

mod contracts;
mod control_flow;
mod definitions;
mod error_handling;
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
        observe_stage(pipeline, phases::SEMANTIC_CONTROL_FLOW, || {
            self.stage3_control_flow_and_patterns(ctx, &program);
        });

        let Some(resolution) = observe_stage_optional(pipeline, phases::SEMANTIC_NAME_RESOLUTION, || {
            self.stage1_name_resolution(ctx, &program)
        }) else {
            return;
        };

        observe_stage(pipeline, phases::SEMANTIC_VISIBILITY, || {
            self.stage5_modules_and_visibility(ctx, &program);
        });
        observe_stage(pipeline, phases::SEMANTIC_CONTRACTS, || {
            self.stage6_contracts_and_methods(ctx, &program, &resolution);
        });
        observe_stage(pipeline, phases::SEMANTIC_ERROR_HANDLING, || {
            self.stage7_error_handling(ctx, &program, &resolution);
        });
        observe_stage(pipeline, phases::SEMANTIC_TYPE_CHECK, || {
            self.stage2_type_check(ctx, &program);
        });
        observe_stage(pipeline, phases::SEMANTIC_NAMING_STYLE, || {
            self.stage_naming_style(ctx, program);
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
