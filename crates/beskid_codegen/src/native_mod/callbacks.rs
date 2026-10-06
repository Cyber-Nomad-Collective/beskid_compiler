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
    pub(super) fn for_selected_closure(input: &CodegenInput<'_>, selected: &[AstNodeKey]) -> anyhow::Result<Self> {
        let mut wrappers = HashSet::new();
        let mut callbacks = Vec::new();
        let mut visited = HashSet::new();
        let mut pending = selected.iter().copied().map(|key| (key, 0usize)).collect::<Vec<_>>();
        while let Some((key, depth)) = pending.pop() {
            if depth > 1024 {
                anyhow::bail!("native SDK callback closure exceeds depth budget");
            }
            if !visited.insert(key) {
                continue;
            }
            if visited.len() > 1_000_000 {
                anyhow::bail!("native SDK callback closure exceeds traversal budget");
            }
            if let Some(lowering) = call_lowering(input.database(), key)? {
                match lowering {
                    CallLowering::NativeModCallback(callback) => {
                        let wrapper = callback.wrapper();
                        super::authority::require_sdk_package(input, wrapper)?;
                        if wrappers.insert(wrapper) { callbacks.push(callback); }
                    }
                    // Follow canonical resolved callees as well as syntax children.
                    // A callback used through a helper must retain the same issuer
                    // checks; a helper's spelling cannot grant callback authority.
                    CallLowering::Direct(callee) => {
                        if callee.generation != input.typed_program().generation {
                            anyhow::bail!("native SDK callback closure contains a stale callee");
                        }
                        pending.push((callee, depth + 1));
                    }
                    _ => {}
                }
            }
            if let Some(children) = child_nodes(input.database(), key)? {
                pending.extend(children.iter().copied().map(|child| (child, depth + 1)));
            }
        }
        Ok(Self { generation: input.typed_program().generation, wrappers: Arc::new(wrappers), callbacks: Arc::new(callbacks) })
    }
    pub(super) fn callbacks(&self) -> impl Iterator<Item=NativeModCallback> + '_ { self.callbacks.iter().copied() }
    pub(super) fn wrappers(&self) -> impl Iterator<Item=AstNodeKey> + '_ { self.wrappers.iter().copied() }
    pub(crate) fn authorizes(&self, callback: NativeModCallback) -> bool {
        callback.wrapper().generation == self.generation && self.wrappers.contains(&callback.wrapper())
    }
}
