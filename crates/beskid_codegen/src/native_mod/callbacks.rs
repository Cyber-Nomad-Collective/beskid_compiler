//! Private executable callback admission, separate from immutable SDK correspondence.
use crate::CodegenInput;
use beskid_queries::{AstNodeKey, CallLowering, NativeModCallback, call_lowering, child_nodes};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone)]
pub(crate) struct NativeModCallbackCapability {
    generation: beskid_queries::SyntaxGenerationId,
    wrappers: Arc<HashSet<AstNodeKey>>,
    callbacks: Arc<Vec<NativeModCallback>>,
}
impl NativeModCallbackCapability {
    /// Called only by the prepared native adapter issuer after exact SDK/source layout
    /// and contract witness selection. Never exposed as a caller construction API.
    ///
    /// The admitted callbacks are exactly those called inside the bodies that module emission
    /// lowers for the selected callables: their reachable source closure, expanded by the same
    /// specialization, contract-witness and spawn resolution (`emitted_item_keys`). A callback
    /// reached only through a witness (`S: ShapeSource` with `S = HostShapes`) is therefore
    /// admitted, and nothing outside the emitted bodies is.
    pub(super) fn for_selected_closure(
        input: &CodegenInput<'_>,
        selected: &[AstNodeKey],
        items: &[crate::SyntaxModuleItem],
    ) -> anyhow::Result<Self> {
        use anyhow::Context;
        let generation = input.typed_program().generation;
        let mut reachable = HashSet::new();
        for key in selected.iter().copied() {
            let root = input
                .roots()
                .iter()
                .copied()
                .find(|root| root.unit == key.unit)
                .context("native callable has no registered source root")?;
            let closure = beskid_queries::reachable_items(input.database(), root, key)?
                .with_context(|| {
                    format!(
                        "native callable reachable facts are incomplete at {}",
                        beskid_queries::format_ast_node_site(input.database(), key)
                    )
                })?;
            reachable.extend(closure.iter().copied());
            reachable.insert(key);
        }
        let source_items = items.iter().filter(|item| reachable.contains(&item.key)).cloned().collect::<Vec<_>>();
        let emitted = crate::module_emission::emitted_item_keys(input, &source_items)
            .map_err(|error| anyhow::anyhow!("native callback closure resolution failed: {error}"))?;
        let mut wrappers = HashSet::new();
        let mut callbacks = Vec::new();
        let mut visited = HashSet::new();
        let mut pending = emitted.into_iter().map(|key| (key, 0usize)).collect::<Vec<_>>();
        while let Some((key, depth)) = pending.pop() {
            if key.generation != generation {
                anyhow::bail!("native SDK callback closure contains a stale item");
            }
            if depth > 4096 {
                anyhow::bail!("native SDK callback closure exceeds depth budget");
            }
            if !visited.insert(key) {
                continue;
            }
            if visited.len() > 1_000_000 {
                anyhow::bail!("native SDK callback closure exceeds traversal budget");
            }
            // A source callback always classifies first in `path_call_resolution`; every other
            // call form is lowered by its own fact and is not a callback.
            if let Ok(Some(CallLowering::NativeModCallback(callback))) = call_lowering(input.database(), key) {
                let wrapper = callback.wrapper();
                super::authority::require_sdk_package(input, wrapper)?;
                if wrappers.insert(wrapper) {
                    callbacks.push(callback);
                }
            }
            if let Some(children) = child_nodes(input.database(), key)? {
                pending.extend(children.iter().copied().map(|child| (child, depth + 1)));
            }
        }
        Ok(Self { generation, wrappers: Arc::new(wrappers), callbacks: Arc::new(callbacks) })
    }
    pub(super) fn callbacks(&self) -> impl Iterator<Item=NativeModCallback> + '_ { self.callbacks.iter().copied() }
    pub(super) fn wrappers(&self) -> impl Iterator<Item=AstNodeKey> + '_ { self.wrappers.iter().copied() }
    pub(crate) fn authorizes(&self, callback: NativeModCallback) -> bool {
        callback.wrapper().generation == self.generation && self.wrappers.contains(&callback.wrapper())
    }
}
