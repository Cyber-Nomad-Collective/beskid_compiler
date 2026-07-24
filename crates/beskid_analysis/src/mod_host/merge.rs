use anyhow::Result;

use crate::syntax::{Program, Spanned};

use super::query_bridge::{QueryBounds, SdkNodeRef, SdkSyntaxPipeline, materialize_snapshot};
use super::types::GeneratedSyntax;

pub(crate) fn merge_generated_syntax(
    program: Spanned<Program>,
    generated: &GeneratedSyntax,
) -> Result<Spanned<Program>> {
    if !generated.has_typed_merge() {
        return Ok(program);
    }

    let mut merged = program;
    for item in &generated.typed_items {
        merged.node.items.push(item.clone());
        merged.node.leading_docs.push(None);
    }

    if !generated.pipeline_ops.is_empty() {
        const GENERATION_ID: u64 = 1;
        let ops = {
            let snapshot = materialize_snapshot(&merged, GENERATION_ID);
            let pipeline = SdkSyntaxPipeline::from_ops(
                &snapshot,
                SdkNodeRef { syntax_generation_id: GENERATION_ID, node_id: snapshot.root_id() },
                QueryBounds { max_nodes: 0, max_depth: 0 },
                generated.pipeline_ops.clone(),
            );
            pipeline.validate().map_err(|err| anyhow::anyhow!("failed to validate mod pipeline ops: {err:?}"))?;
            pipeline.ordered_ops()
        };
        for op in ops {
            super::query_bridge::apply_program_item_op(&mut merged, GENERATION_ID, op)
                .map_err(|err| anyhow::anyhow!("failed to apply mod pipeline ops: {err:?}"))?;
        }
    }

    Ok(merged)
}
