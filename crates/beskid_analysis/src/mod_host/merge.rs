use anyhow::Result;

use crate::syntax::{Program, Spanned};

use super::query_bridge::{QueryBounds, SdkNodeRef, SdkSyntaxPipeline, materialize_snapshot};
use super::types::{GeneratedItemLineage, GeneratedSyntax};

/// The merged program together with the lineage of every item the merge appended.
pub(crate) struct MergedSyntax {
    pub program: Spanned<Program>,
    pub lineage: Vec<GeneratedItemLineage>,
}

/// Append admitted generated items to `program` and apply pipeline operations.
///
/// Items and their origin lineage are validated together before anything is appended: the
/// lineage must align one-to-one with the items and every origin must be the actual syntax
/// generation this invocation was issued. A mismatch publishes nothing.
pub(crate) fn merge_generated_syntax(
    program: Spanned<Program>,
    generated: &GeneratedSyntax,
    generation: Option<crate::syntax::SyntaxGenerationId>,
) -> Result<MergedSyntax> {
    if !generated.has_typed_merge() {
        return Ok(MergedSyntax { program, lineage: Vec::new() });
    }
    if generated.typed_items.len() != generated.provenance.len() {
        anyhow::bail!(
            "generated syntax carries {} items but {} origin lineages",
            generated.typed_items.len(),
            generated.provenance.len()
        );
    }
    if !generated.typed_items.is_empty() {
        let issued = generation
            .filter(|generation| generation.0 != 0)
            .ok_or_else(|| anyhow::anyhow!("generated syntax requires actual syntax generation authority"))?;
        if let Some(foreign) = generated.provenance.iter().find(|origin| origin.origin_generation != issued.0) {
            anyhow::bail!(
                "generated item from `{}` originates in generation {}, not the issued generation {}",
                foreign.generator_type_id,
                foreign.origin_generation,
                issued.0
            );
        }
    }

    let mut merged = program;
    let mut lineage = Vec::with_capacity(generated.typed_items.len());
    let mut cursor = synthetic_span_origin(&merged);
    let mut appended = Vec::with_capacity(generated.typed_items.len());
    for (item, provenance) in generated.typed_items.iter().zip(&generated.provenance) {
        let item = respan_generated_item(item, &mut cursor)?;
        lineage.push(GeneratedItemLineage { merged_span: item.span, provenance: provenance.clone() });
        appended.push(item);
    }
    for item in appended {
        merged.node.items.push(item);
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

    Ok(MergedSyntax { program: merged, lineage })
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

    fn origin(generation: u64, span: crate::syntax::SpanInfo) -> crate::mod_host::StructuralProvenance {
        crate::mod_host::StructuralProvenance {
            source_name: "Main.bd".into(),
            origin_generation: generation,
            generator_type_id: "Fixture.Generator".into(),
            origin_span: span,
        }
    }

    fn generated_function(source: &str) -> Vec<Spanned<crate::mod_host::types::ProgramItem>> {
        parse_program_with_source_name("Gen.bd", source).expect("parse generated").node.items
    }

    /// Lineage is validated with its items before anything is appended: a missing lineage,
    /// an absent issued generation or a foreign origin generation publish nothing.
    #[test]
    fn typed_merge_rejects_unaligned_absent_and_foreign_lineage_atomically() {
        let host = parse_program_with_source_name("Main.bd", "unit Main() { return; }").expect("parse host");
        let items = generated_function("pub unit First() { return; }\npub unit Second() { return; }");
        let issued = Some(crate::syntax::SyntaxGenerationId(7));

        let unaligned = GeneratedSyntax {
            provenance: vec![origin(7, items[0].span)],
            typed_items: items.clone(),
            ..Default::default()
        };
        assert!(merge_generated_syntax(host.clone(), &unaligned, issued).is_err());

        let aligned: Vec<_> = items.iter().map(|item| origin(7, item.span)).collect();
        let complete = GeneratedSyntax { typed_items: items.clone(), provenance: aligned, ..Default::default() };
        assert!(merge_generated_syntax(host.clone(), &complete, None).is_err(), "absent generation authority");
        assert!(
            merge_generated_syntax(host.clone(), &complete, Some(crate::syntax::SyntaxGenerationId(0))).is_err(),
            "generation zero is not authority"
        );

        let foreign = GeneratedSyntax {
            provenance: vec![origin(7, items[0].span), origin(8, items[1].span)],
            typed_items: items.clone(),
            ..Default::default()
        };
        assert!(merge_generated_syntax(host.clone(), &foreign, issued).is_err());

        let merged = merge_generated_syntax(host.clone(), &complete, issued).expect("aligned lineage merges");
        assert_eq!(merged.program.node.items.len(), host.node.items.len() + 2);
        assert_eq!(merged.lineage.len(), 2);
        assert_eq!(merged.lineage[0].merged_span, merged.program.node.items[1].span);
        assert_eq!(merged.lineage[1].merged_span, merged.program.node.items[2].span);
        assert_eq!(merged.lineage[1].provenance.origin_span, items[1].span);
    }

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
        let provenance = generated_items.iter().map(|item| origin(7, item.span)).collect();
        let generated = GeneratedSyntax { typed_items: generated_items, provenance, ..Default::default() };
        let host_end = synthetic_span_origin(&host);

        let merged = merge_generated_syntax(host, &generated, Some(crate::syntax::SyntaxGenerationId(7)))
            .expect("merge");
        assert_eq!(merged.lineage.len(), 2, "every appended item retains its origin lineage");
        assert!(merged.lineage.iter().all(|lineage| lineage.provenance.origin_generation == 7));
        let merged = merged.program;
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
