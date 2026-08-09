use std::collections::{HashMap, HashSet};

use beskid_isle::{AstNodeKey, DirectCallee};
use beskid_queries::{
    CallLowering, GenericSpecializationInstance, SemanticTypeId, call_lowering, child_nodes, format_ast_node_key,
    generic_call_specialization, generic_specialization_identity, item_name, node_kind, node_span,
};

use crate::CodegenInput;

/// Trace only facts already read by the syntax-only lowering boundary. This has no bearing on
/// selection; it makes every unavailable fact explicit instead of making a HIR-era guess.
pub(super) fn trace_item_facts(
    input: &CodegenInput<'_>,
    item: AstNodeKey,
    symbols: &HashMap<DirectCallee, String>,
) {
    if !crate::isle_trace::enabled() {
        return;
    }
    let db = input.database();
    let mut visited = HashSet::new();
    trace_node_facts(db, item, symbols, &mut visited);
}

fn trace_node_facts(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    symbols: &HashMap<DirectCallee, String>,
    visited: &mut HashSet<AstNodeKey>,
) {
    if !visited.insert(key) {
        return;
    }
    let node = trace_key(db, key);
    let kind =
        node_kind(db, key).ok().flatten().map(|kind| format!("{kind:?}")).unwrap_or_else(|| "<missing>".to_owned());
    let span = node_span(db, key)
        .ok()
        .flatten()
        .map(|span| {
            format!(
                "{}:{}-{}:{} bytes={}-{}",
                span.line_col_start.0,
                span.line_col_start.1,
                span.line_col_end.0,
                span.line_col_end.1,
                span.start,
                span.end
            )
        })
        .unwrap_or_else(|| "<missing>".to_owned());
    crate::isle_trace::event(|| format!("event=ast.node key={node} kind={kind} span={span}"));

    if let Ok(Some(lowering)) = call_lowering(db, key) {
        let (lowering_name, callee) = match lowering {
            CallLowering::Direct(declaration) => {
                let callee = generic_call_specialization(db, key)
                    .ok()
                    .flatten()
                    .map(|specialization| {
                        DirectCallee::specialized_item(
                            specialization.declaration,
                            generic_specialization_identity(&GenericSpecializationInstance {
                                declaration: specialization.declaration,
                                signature: specialization.signature.clone(),
                                substitutions: specialization.substitutions.clone(),
                            }),
                        )
                    })
                    .unwrap_or_else(|| DirectCallee::item(declaration));
                ("Direct", Some(callee))
            }
            CallLowering::Dynamic => ("Dynamic", None),
            CallLowering::Runtime(intrinsic) => ("Runtime", Some(DirectCallee::runtime_intrinsic(intrinsic.0))),
            CallLowering::CorelibService(service) => {
                ("CorelibService", Some(DirectCallee::corelib_service(service.symbol)))
            }
        };
        match callee {
            Some(callee) => {
                let import = symbols.get(&callee).map(String::as_str).unwrap_or("<missing>");
                let callee_display = format_callee_for_trace(db, &callee);
                crate::isle_trace::event(|| {
                    format!(
                        "event=call.fact key={node} lowering={lowering_name} callee={callee_display} module_import={import}"
                    )
                });
            }
            None => crate::isle_trace::event(|| {
                format!(
                    "event=call.fact key={node} lowering={lowering_name} callee=<unavailable> module_import=<none>"
                )
            }),
        }
    }

    match child_nodes(db, key) {
        Ok(Some(children)) => {
            for child in children.iter().copied() {
                trace_node_facts(db, child, symbols, visited);
            }
        }
        Ok(None) => {
            crate::isle_trace::event(|| format!("event=isle.missing rule=child_nodes key={node} detail=unavailable"))
        }
        Err(error) => {
            crate::isle_trace::event(|| format!("event=isle.missing rule=child_nodes key={node} detail={error}"))
        }
    }
}

pub(super) fn trace_key(db: &dyn beskid_queries::Db, key: AstNodeKey) -> String {
    format_ast_node_key(db, key)
}

fn format_abi_identity(identity: &[u32]) -> String {
    if identity.is_empty() {
        return "[]".to_owned();
    }
    let (parameters, result) = identity.split_at(identity.len() - 1);
    let parameters =
        parameters.iter().copied().map(|id| SemanticTypeId(id).display_name()).collect::<Vec<_>>().join(", ");
    let result =
        result.first().copied().map(|id| SemanticTypeId(id).display_name()).unwrap_or_else(|| "unit".to_owned());
    format!("[{parameters}]->{result}")
}

pub(super) fn format_declaration_for_trace(db: &dyn beskid_queries::Db, key: AstNodeKey) -> String {
    let name = item_name(db, key).ok().flatten().map(|name| name.to_string()).unwrap_or_else(|| "<anon>".to_owned());
    format!("{name}@{}", trace_key(db, key))
}

fn format_callee_for_trace(db: &dyn beskid_queries::Db, callee: &DirectCallee) -> String {
    match callee {
        DirectCallee::Item(key) => format!("Item({})", format_declaration_for_trace(db, *key)),
        DirectCallee::SpecializedItem { declaration, abi_identity } => format!(
            "SpecializedItem({}, {})",
            format_declaration_for_trace(db, *declaration),
            format_abi_identity(abi_identity)
        ),
        DirectCallee::RuntimeIntrinsic(index) => format!("RuntimeIntrinsic({index})"),
        DirectCallee::CorelibService(symbol) => format!("CorelibService({symbol})"),
        DirectCallee::SpawnTrampoline(spawn) => {
            format!("SpawnTrampoline({})", trace_key(db, *spawn))
        }
        DirectCallee::LambdaTrampoline(lambda) => {
            format!("LambdaTrampoline({})", trace_key(db, *lambda))
        }
    }
}
