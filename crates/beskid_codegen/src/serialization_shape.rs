//! Immutable serialization metadata issued from retained typed contributions.
//! Descriptive source packets, hashes, and field names cannot construct this binding.
use crate::{CodegenInput, stable_shape::CompiledStableShape};
use beskid_queries::{AstNodeKey, CompiledCatchallBinding, CompiledSerializationTarget, ReboundCatchallBinding};

#[derive(Clone)]
pub struct CompiledSerializationShape {
    kind: beskid_queries::SerializationContributionKind,
    owner: AstNodeKey,
    implementation: AstNodeKey,
    shape: CompiledStableShape,
    extras: Option<ReboundCatchallBinding>,
}
impl CompiledSerializationShape {
    pub fn kind(&self) -> beskid_queries::SerializationContributionKind {
        self.kind
    }
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
        let mut emitted = Vec::new();
        for target in targets {
            let rebound = target.rebind(self.database(), program).map_err(|error| error.to_string())?;
            if emitted.iter().any(|issued: &CompiledSerializationShape| {
                issued.owner == rebound.owner() && issued.kind == rebound.kind()
            }) {
                return Err("duplicate current serialization contribution target".into());
            }
            let graph = rebound.shape(self.database(), program).map_err(|error| error.to_string())?;
            let shape = self.compile_stable_shape_graph(&graph, schema_version, max_signature_bytes)?;
            let mut extras = None;
            for catchall in &catchalls {
                // Distinct source units/owner declarations are not silently treated as
                // catchall claims for this target. Matched claims must all revalidate.
                if catchall.owner().unit != rebound.owner().unit {
                    continue;
                }
                if let Ok(binding) = catchall.rebind(self.database(), program, &rebound) {
                    if extras.replace(binding).is_some() {
                        return Err("duplicate catchall for current serialization target".into());
                    }
                } else if catchall.owner().node == target.original_owner().node {
                    return Err("serialization catchall correspondence changed".into());
                }
            }
            emitted.push(CompiledSerializationShape {
                kind: rebound.kind(),
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
