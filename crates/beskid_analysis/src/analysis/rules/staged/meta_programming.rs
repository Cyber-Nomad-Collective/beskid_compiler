//! Stage 8 — metaprogramming surface law for the **host semantic snapshot**.
//!
//! ## Host bridge (platform-spec)
//!
//! - **Contributor vs inspector** roles, bounded scheduling, and **atomic `meta.round_commit`**
//!   merges are enforced by the compiler host *before* this snapshot runs; this stage only sees
//!   the merged [`crate::hir::HirProgram`] for the active generation (see
//!   `meta.host_attached` / `meta.round_*` phase ids in `beskid_pipeline::phases`).
//! - **Capabilities** exposed to future Beskid meta entrypoints are the closed set in
//!   [`crate::projects::META_CAPABILITY_NAMES`] (`diagnostics`, `read_project_sources`,
//!   `emit_syntax`, `query_semantic_snapshot`, `extern_ffi`); unknown manifest entries are
//!   rejected earlier as [`crate::projects::ProjectError::MetaContractViolation`].

use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::hir::{HirItem, HirProgram};
use crate::syntax::Spanned;

impl SemanticPipelineRule {
    pub(super) fn stage8_metaprogramming(&self, ctx: &mut RuleContext, hir: &Spanned<HirProgram>) {
        let Some(allowed) = ctx.options.module_level_meta_items_allowed else {
            return;
        };
        if allowed {
            return;
        }
        diagnose_forbidden_meta_module_items(ctx, &hir.node.items);
    }
}

fn diagnose_forbidden_meta_module_items(ctx: &mut RuleContext, items: &[Spanned<HirItem>]) {
    for item in items {
        match &item.node {
            HirItem::MetaDefinition(def) => {
                let name = def.node.name.node.name.clone();
                ctx.emit_issue(
                    def.node.name.span,
                    SemanticIssueKind::ForbiddenMetaModuleItem { name },
                );
            }
            HirItem::InlineModule(im) => {
                diagnose_forbidden_meta_module_items(ctx, &im.node.items);
            }
            _ => {}
        }
    }
}
