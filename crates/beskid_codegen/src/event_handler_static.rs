//! Static ABI-v5 layout shared by event-handler wrapper objects.

use cranelift_module::{DataDescription, Linkage, Module, ModuleError, ModuleResult};

pub const EVENT_HANDLER_POINTER_MAP_SYMBOL: &str = "__beskid_event_handler_pointer_map_v5";
pub const EVENT_HANDLER_DESCRIPTOR_SYMBOL: &str = "__beskid_event_handler_descriptor_v5";
pub const EVENT_HANDLER_ALLOCATION_REQUEST_SYMBOL: &str = "__beskid_event_handler_allocation_request_v5";
pub const EVENT_HANDLER_CODE_OFFSET: i32 = 16;
pub const EVENT_HANDLER_ENVIRONMENT_OFFSET: i32 = 24;

/// Define the fixed wrapper descriptor: `{ header, code pointer, traced environment pointer }`.
pub fn emit_event_handler_static_data<M: Module>(module: &mut M) -> ModuleResult<()> {
    let pointer_map = module.declare_data(EVENT_HANDLER_POINTER_MAP_SYMBOL, Linkage::Local, false, false)?;
    let descriptor = module.declare_data(EVENT_HANDLER_DESCRIPTOR_SYMBOL, Linkage::Local, false, false)?;
    let request = module.declare_data(EVENT_HANDLER_ALLOCATION_REQUEST_SYMBOL, Linkage::Local, false, false)?;

    let mut pointer_map_data = DataDescription::new();
    pointer_map_data.define((EVENT_HANDLER_ENVIRONMENT_OFFSET as u64).to_le_bytes().to_vec().into_boxed_slice());
    module.define_data(pointer_map, &pointer_map_data)?;

    let mut descriptor_bytes = vec![0u8; 40];
    write_word(&mut descriptor_bytes, 0, 32)?;
    write_word(&mut descriptor_bytes, 8, 8)?;
    write_word(&mut descriptor_bytes, 24, 1)?;
    let mut descriptor_data = DataDescription::new();
    descriptor_data.define(descriptor_bytes.into_boxed_slice());
    let pointer_map_address = module.declare_data_in_data(pointer_map, &mut descriptor_data);
    descriptor_data.write_data_addr(16, pointer_map_address, 0);
    module.define_data(descriptor, &descriptor_data)?;

    let mut request_bytes = vec![0u8; 24];
    write_word(&mut request_bytes, 0, 32)?;
    write_word(&mut request_bytes, 8, 8)?;
    let mut request_data = DataDescription::new();
    request_data.define(request_bytes.into_boxed_slice());
    let descriptor_address = module.declare_data_in_data(descriptor, &mut request_data);
    request_data.write_data_addr(16, descriptor_address, 0);
    module.define_data(request, &request_data)
}

fn write_word(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), ModuleError> {
    let destination = bytes
        .get_mut(offset..offset + 8)
        .ok_or_else(|| ModuleError::Backend(anyhow::anyhow!("event handler static-data layout overflow")))?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(())
}
