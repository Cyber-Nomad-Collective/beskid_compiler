//! Immutable serialization metadata issued from retained typed contributions.
//! Descriptive source packets, hashes, and field names cannot construct this binding.
use crate::{CodegenInput, stable_shape::CompiledStableShape};
use beskid_queries::{AstNodeKey, CompiledCatchallBinding, CompiledSerializationTarget, ReboundCatchallBinding};

#[derive(Clone)]
pub struct CompiledSerializationShape {
    owner: AstNodeKey,
    implementation: AstNodeKey,
    shape: CompiledStableShape,
    extras: Option<ReboundCatchallBinding>,
}
impl CompiledSerializationShape {
    pub fn owner(&self) -> AstNodeKey {
        self.owner
    }
    pub fn implementation(&self) -> AstNodeKey {
        self.implementation
    }
    pub fn shape(&self) -> &CompiledStableShape {
        &self.shape
    }
    pub fn extras(&self) -> Option<&ReboundCatchallBinding> {
        self.extras.as_ref()
    }
}
impl CodegenInput<'_> {
    /// Metadata is parent-owned immutable issuer payload. Each carrier must rebind
    /// through exact appended-contribution correspondence in this current assembly.
    ///
    /// A target's descriptor graph is format-independent, so it is issued from the exact
    /// correspondence of its retained groups, one shape per target declaration. Conformance of a
    /// group generic over its format (`impl<E> Point : Serializable<E>`, the decoder adapter over
    /// `R`) is proven per use in a complete environment (the field codec correspondence).
    pub fn compiled_serialization_shapes(
        &self,
        schema_version: &str,
        max_signature_bytes: usize,
    ) -> Result<Vec<CompiledSerializationShape>, String> {
        let program = self.typed_program();
        let metadata = &program.assembly.compiled_mod_metadata;
        let targets = metadata.iter().filter_map(|metadata| metadata.issuer_payload::<CompiledSerializationTarget>());
        let catchalls = metadata
            .iter()
            .filter_map(|metadata| metadata.issuer_payload::<CompiledCatchallBinding>())
            .collect::<Vec<_>>();
        let mut emitted = Vec::<CompiledSerializationShape>::new();
        let mut implementations = std::collections::HashSet::new();
        for target in targets {
            let rebound = target.corresponding_target(self.database(), program).map_err(|error| error.to_string())?;
            if !implementations.insert(rebound.implementation()) {
                return Err("duplicate current serialization contribution target".into());
            }
            let graph = rebound.shape(self.database(), program).map_err(|error| error.to_string())?;
            let shape = self.compile_stable_shape_graph(&graph, schema_version, max_signature_bytes)?;
            // The encoder and decoder groups of one target issue the same graph once.
            if let Some(issued) = emitted.iter().find(|issued| issued.owner == rebound.owner()) {
                if issued.shape.sha256() != shape.sha256() {
                    return Err("serialization groups of one target issue different graphs".into());
                }
                continue;
            }
            let mut extras = None;
            for catchall in &catchalls {
                // Distinct source units/owner declarations are not silently treated as
                // catchall claims for this target. Matched claims must all revalidate.
                if catchall.owner().path() != rebound.owner().unit.path(self.database()).as_path() {
                    continue;
                }
                if let Ok(binding) = catchall.rebind(self.database(), program, &rebound) {
                    if extras.replace(binding).is_some() {
                        return Err("duplicate catchall for current serialization target".into());
                    }
                } else if catchall.owner() == target.original_owner() {
                    return Err("serialization catchall correspondence changed".into());
                }
            }
            emitted.push(CompiledSerializationShape {
                owner: rebound.owner(),
                implementation: rebound.implementation(),
                shape,
                extras,
            });
        }
        // Every retained catchall must belong to an emitted exact target; orphaned
        // claims do not survive as descriptive runtime permissions.
        for catchall in catchalls {
            if !emitted.iter().any(|shape| {
                shape.extras.as_ref().is_some_and(|extras| {
                    extras.field().source_unit == catchall.field().source_unit
                        && extras.field().span == catchall.field().span
                })
            }) {
                return Err("catchall metadata has no current issued serialization target".into());
            }
        }
        Ok(emitted)
    }
}
