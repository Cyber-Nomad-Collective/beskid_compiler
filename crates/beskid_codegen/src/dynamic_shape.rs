//! Portable Dynamic signatures issued only from a concrete canonical Pack instance.
//! The graph is semantic authority; its source witnesses come from the same TypedProgram.
use crate::CodegenInput;
use beskid_queries::GenericSpecializationInstance;

#[derive(Debug, Clone)]
pub struct CompiledDynamicShape {
    bytes: Vec<u8>,
    sha256: [u8; 32],
}
impl CompiledDynamicShape {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
}

impl CodegenInput<'_> {
    pub fn compiled_dynamic_packing_shape(
        &self,
        instance: &GenericSpecializationInstance,
    ) -> Result<Option<CompiledDynamicShape>, String> {
        if instance.declaration.generation != self.typed_program().generation {
            return Err("Dynamic packing belongs to a different assembly generation".into());
        }
        let Some(shape) =
            beskid_queries::dynamic_packing_shape(self.database(), instance).map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let stable = self.compile_stable_shape_graph(&shape, "1", 1024 * 1024)?;
        Ok(Some(CompiledDynamicShape { bytes: stable.signature().to_vec(), sha256: *stable.sha256() }))
    }
}
