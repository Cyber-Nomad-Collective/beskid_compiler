use anyhow::Result;

use crate::syntax::{Program, Spanned};

use super::query_bridge::{QueryBounds, SdkNodeRef, SdkSyntaxPipeline, materialize_snapshot};
use super::types::GeneratedSyntax;

pub(crate) fn merge_generated_syntax(
    program: Spanned<Program>,
    generated: &GeneratedSyntax,
    generation: Option<crate::syntax::SyntaxGenerationId>,
) -> Result<Spanned<Program>> {
    if !generated.has_typed_merge() {
        return Ok(program);
    }

    let mut merged = program;
    let mut cursor = synthetic_span_origin(&merged);
    for item in &generated.typed_items {
        merged.node.items.push(respan_generated_item(item, &mut cursor)?);
        merged.node.leading_docs.push(None);
    }

    if !generated.pipeline_ops.is_empty() {
        let generation_id = generation
            .filter(|generation| generation.0 != 0)
            .ok_or_else(|| anyhow::anyhow!("mod pipeline operations require actual syntax generation authority"))?
            .0;
        let ops = {
            let snapshot = materialize_snapshot(&merged, generation_id);
            let pipeline = SdkSyntaxPipeline::from_ops(
                &snapshot,
                SdkNodeRef { syntax_generation_id: generation_id, node_id: snapshot.root_id() },
                QueryBounds { max_nodes: 0, max_depth: 0 },
                generated.pipeline_ops.clone(),
            );
            pipeline.validate().map_err(|err| anyhow::anyhow!("failed to validate mod pipeline ops: {err:?}"))?;
            pipeline.ordered_ops()
        };
        for op in ops {
            super::query_bridge::apply_program_item_op(&mut merged, generation_id, op)
                .map_err(|err| anyhow::anyhow!("failed to apply mod pipeline ops: {err:?}"))?;
        }
    }

    Ok(merged)
}

/// First byte of the synthetic span region: one past every byte the host unit's own syntax
/// covers, so no generated span can alias a host span.
fn synthetic_span_origin(program: &Spanned<Program>) -> usize {
    program
        .node
        .items
        .iter()
        .map(|item| item.span.end)
        .chain(std::iter::once(program.span.end))
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

/// Give every node of one generated contribution a distinct, properly nested synthetic span.
///
/// Generated syntax carries no source positions: a contribution arrives with the empty span
/// `0..0` on every node. Resolution and type-checking tables of this crate are keyed by span,
/// so two generated declarations (or two nodes inside one) would alias each other and a call
/// into a generated function would find no signature. Re-spanning assigns each node a byte range
/// of its own, nested inside its parent's, in tree order, starting at `cursor`. Line/column
/// positions are left as the generator reported them: generated code has no source line.
///
/// The walk runs over the node's serialized form, the one representation that reaches every
/// `Spanned<_>` of every node kind uniformly; the syntax types themselves offer no mutable
/// traversal.
fn respan_generated_item(
    item: &Spanned<super::types::ProgramItem>,
    cursor: &mut usize,
) -> Result<Spanned<super::types::ProgramItem>> {
    let mut value = serde_json::to_value(item)?;
    assign_synthetic_spans(&mut value, cursor);
    Ok(serde_json::from_value(value)?)
}

fn assign_synthetic_spans(value: &mut serde_json::Value, cursor: &mut usize) {
    use serde_json::Value;
    match value {
        Value::Object(object) => {
            let spanned = object.contains_key("node") && object.contains_key("span") && object.contains_key("id");
            if !spanned {
                object.values_mut().for_each(|value| assign_synthetic_spans(value, cursor));
                return;
            }
            let start = *cursor;
            *cursor += 1;
            object.iter_mut().filter(|(key, _)| *key != "span").for_each(|(_, value)| {
                assign_synthetic_spans(value, cursor);
            });
            let end = *cursor;
            *cursor += 1;
            if let Some(Value::Object(span)) = object.get_mut("span") {
                span.insert("start".into(), Value::from(start));
                span.insert("end".into(), Value::from(end));
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|value| assign_synthetic_spans(value, cursor)),
        _ => {}
    }
}

#[cfg(test)]
mod respan_tests {
    use super::*;
    use crate::services::parse_program_with_source_name;

    fn spans(value: &serde_json::Value, out: &mut Vec<(usize, usize)>) {
        match value {
            serde_json::Value::Object(object) => {
                if object.contains_key("node") && object.contains_key("span") && object.contains_key("id") {
                    let span = &object["span"];
                    out.push((span["start"].as_u64().unwrap() as usize, span["end"].as_u64().unwrap() as usize));
                }
                object.values().for_each(|value| spans(value, out));
            }
            serde_json::Value::Array(items) => items.iter().for_each(|value| spans(value, out)),
            _ => {}
        }
    }

    /// Generated declarations arrive with every node at `0..0`; after the merge each node owns a
    /// distinct span nested inside its parent's and past the host unit, so span-keyed resolution
    /// and signature tables keep one entry per generated node.
    #[test]
    fn merged_generated_items_receive_distinct_nested_synthetic_spans() {
        let host = parse_program_with_source_name("Main.bd", "unit Main() { return; }").expect("parse host");
        let generated_source = "pub unit GeneratedUnit() { return; }\npub type SerializedRecord<T> { pub T value }";
        let generated_items = parse_program_with_source_name("Gen.bd", generated_source)
            .expect("parse generated")
            .node
            .items
            .into_iter()
            .map(|item| {
                let mut value = serde_json::to_value(&item).unwrap();
                fn clear(value: &mut serde_json::Value) {
                    match value {
                        serde_json::Value::Object(object) => {
                            if let Some(serde_json::Value::Object(span)) = object.get_mut("span") {
                                span.insert("start".into(), 0.into());
                                span.insert("end".into(), 0.into());
                            }
                            object.values_mut().for_each(clear);
                        }
                        serde_json::Value::Array(items) => items.iter_mut().for_each(clear),
                        _ => {}
                    }
                }
                clear(&mut value);
                serde_json::from_value::<Spanned<crate::mod_host::types::ProgramItem>>(value).unwrap()
            })
            .collect::<Vec<_>>();
        assert!(generated_items.iter().all(|item| item.span.start == 0 && item.span.end == 0));
        let generated = GeneratedSyntax { typed_items: generated_items, ..Default::default() };
        let host_end = synthetic_span_origin(&host);

        let merged = merge_generated_syntax(host, &generated, None).expect("merge");
        let [_, function, record] = merged.node.items.as_slice() else { panic!("two generated items expected") };
        assert!(function.span.start >= host_end && record.span.start > function.span.end, "{function:?} {record:?}");
        let mut all = Vec::new();
        spans(&serde_json::to_value(function).unwrap(), &mut all);
        spans(&serde_json::to_value(record).unwrap(), &mut all);
        let mut unique = all.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), all.len(), "every generated node owns one span: {all:?}");
        assert!(all.iter().all(|(start, end)| start < end && *start >= host_end), "{all:?}");
        let items = [(function.span.start, function.span.end), (record.span.start, record.span.end)];
        assert!(
            all.iter().filter(|span| !items.contains(span)).all(|(start, end)| {
                // Every nested span lies strictly inside one of the two item spans.
                items.iter().any(|(outer_start, outer_end)| outer_start < start && end < outer_end)
            }),
            "{all:?}"
        );
    }
}
